use super::*;
use russh::{
    MethodKind, client,
    keys::{self, PrivateKeyWithHashAlg},
};
use tokio::sync::Mutex;
#[derive(Clone)]
pub struct Challenges {
    pub events: mpsc::Sender<ConnectionEvent>,
    pub answers: Arc<Mutex<mpsc::Receiver<ChallengeAnswer>>>,
    pub lifecycle: Arc<super::lifecycle::ConnectionLifecycle>,
    pub connection: u64,
    pub(super) context: Option<AuthContext>,
}
#[derive(Clone, Copy)]
pub(super) enum HopRole {
    Gateway,
    Target,
}
impl HopRole {
    pub fn title(self, session: &Session) -> String {
        format!(
            "{} SSH {}:{}",
            match self {
                Self::Gateway => "Gateway",
                Self::Target => "Target",
            },
            session.host,
            session.port
        )
    }
}
#[derive(Clone)]
pub(super) struct AuthContext {
    host: String,
    port: u16,
    role: HopRole,
    profile: String,
    reference: Option<String>,
    gateway_refs: std::collections::BTreeMap<String, String>,
    verified_key: Arc<std::sync::Mutex<Option<String>>>,
}
pub fn profile_binding(session: &Session) -> String {
    use sha2::{Digest, Sha256};
    let legacy = serde_json::to_value(&(
        session.host.as_str(),
        session.port,
        session.user.as_str(),
        session.key.as_str(),
        session.jump.as_str(),
        session.ssh_engine,
    ))
    .unwrap();
    let bytes = if session.agent_mode == crate::model::AgentMode::System {
        serde_json::to_vec(&legacy).unwrap()
    } else {
        serde_json::to_vec(&(legacy, session.agent_mode)).unwrap()
    };
    let bytes = if let Some(context) = session.ssh_options.credential_context() {
        serde_json::to_vec(&(bytes, context)).unwrap()
    } else {
        bytes
    };
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
impl Challenges {
    pub(super) fn for_hop(
        &self,
        session: &Session,
        role: HopRole,
        profile: String,
        reference: Option<String>,
        verified_key: Arc<std::sync::Mutex<Option<String>>>,
    ) -> Self {
        let mut bound = self.clone();
        bound.context = Some(AuthContext {
            host: session.host.clone(),
            port: session.port,
            role,
            profile,
            reference,
            gateway_refs: session.gateway_credential_refs.clone(),
            verified_key,
        });
        bound
    }
    fn credential(
        &self,
        user: &str,
        kind: crate::credentials::SshSecretKind,
    ) -> Option<ChallengeCredential> {
        let context = self.context.as_ref()?;
        let host_key = context
            .verified_key
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()?;
        let binding = crate::credentials::CredentialBinding::Ssh {
            role: match context.role {
                HopRole::Target => crate::credentials::CredentialRole::Target,
                HopRole::Gateway => crate::credentials::CredentialRole::Gateway,
            },
            host: context.host.clone(),
            port: context.port,
            user: user.into(),
            host_key,
            profile: context.profile.clone(),
            kind,
        };
        binding.validate().ok()?;
        Some(ChallengeCredential {
            reference: if matches!(context.role, HopRole::Gateway) {
                context
                    .gateway_refs
                    .get(&gateway_reference_key(&context.host, context.port, user))
                    .cloned()
                    .or_else(|| context.reference.clone())
            } else {
                context.reference.clone()
            },
            binding,
        })
    }

    pub async fn ask(
        &self,
        title: String,
        prompts: Vec<Prompt>,
        host_key: bool,
    ) -> Result<ChallengeAnswer, String> {
        self.ask_typed(title, prompts, host_key, None).await
    }
    async fn ask_typed(
        &self,
        title: String,
        prompts: Vec<Prompt>,
        host_key: bool,
        credential: Option<ChallengeCredential>,
    ) -> Result<ChallengeAnswer, String> {
        let id = self.lifecycle.begin_prompt()?;
        self.events
            .send(ConnectionEvent::Challenge {
                connection: self.connection,
                id,
                title,
                prompts,
                host_key,
                credential,
            })
            .await
            .map_err(|_| "Connection closed".to_string())?;
        let mut answers = self.answers.lock().await;
        while let Some(answer) = answers.recv().await {
            if answer.id == id {
                self.lifecycle.consume(id)?;
                return Ok(answer);
            }
        }
        Err("Authentication cancelled".into())
    }
    pub async fn connected_values<H: client::Handler + Send + 'static>(
        &self,
        handle: &mut client::Handle<H>,
        title: String,
        prompts: Vec<Prompt>,
        credential: Option<ChallengeCredential>,
    ) -> Result<Vec<Zeroizing<String>>, String> {
        tokio::select! { biased;
            _ = &mut *handle => { self.lifecycle.cancel(); Err("SSH transport closed while waiting for authentication input".into()) },
            values = self.values(title,prompts,credential) => values,
        }
    }
    pub(super) async fn proxy_values(
        &self,
        title: String,
        credential: ChallengeCredential,
    ) -> Result<Vec<Zeroizing<String>>, String> {
        self.values(
            title,
            vec![
                Prompt {
                    label: "Proxy username".into(),
                    echo: true,
                },
                Prompt {
                    label: "Proxy password".into(),
                    echo: false,
                },
            ],
            Some(credential),
        )
        .await
    }
    pub async fn values(
        &self,
        title: String,
        prompts: Vec<Prompt>,
        credential: Option<ChallengeCredential>,
    ) -> Result<Vec<Zeroizing<String>>, String> {
        let answer = self.ask_typed(title, prompts, false, credential).await?;
        if answer.trust == Some(TrustDecision::Reject) {
            return Err("Authentication cancelled".into());
        }
        Ok(answer.values)
    }
}
fn update_methods(result: client::AuthResult, remaining: &mut russh::MethodSet) -> bool {
    match result {
        client::AuthResult::Success => true,
        client::AuthResult::Failure {
            remaining_methods, ..
        } => {
            *remaining = remaining_methods;
            false
        }
    }
}
pub async fn authenticate<H: client::Handler + Send + 'static>(
    handle: &mut client::Handle<H>,
    s: &Session,
    prompts: &Challenges,
    role: HopRole,
    agent_endpoint: Option<&crate::credentials::agent::AgentEndpoint>,
) -> Result<String, String> {
    let endpoint = role.title(s);
    let user = if s.user.is_empty() {
        prompts
            .connected_values(
                handle,
                format!("{endpoint} · Username"),
                vec![Prompt {
                    label: "Username".into(),
                    echo: true,
                }],
                None,
            )
            .await?
            .first()
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .ok_or("A username is required")?
    } else {
        s.user.clone()
    };
    prompts.lifecycle.ensure_active()?;
    let result = handle
        .authenticate_none(user.clone())
        .await
        .map_err(|e| e.to_string())?;
    if result.success() {
        return Ok(user);
    }
    let mut remaining = match result {
        russh::client::AuthResult::Failure {
            remaining_methods, ..
        } => remaining_methods,
        _ => unreachable!(),
    };
    if remaining.contains(&MethodKind::PublicKey) {
        if !s.key.is_empty() {
            let mut passphrase = None;
            let mut key = keys::load_secret_key(&s.key, None);
            if key.is_err() {
                let values = prompts
                    .connected_values(
                        handle,
                        format!("{endpoint} · Unlock private key as {user}"),
                        vec![Prompt {
                            label: "Private-key passphrase".into(),
                            echo: false,
                        }],
                        key_digest(&s.key).and_then(|key_digest| {
                            prompts.credential(
                                &user,
                                crate::credentials::SshSecretKind::KeyPassphrase { key_digest },
                            )
                        }),
                    )
                    .await?;
                passphrase = values.into_iter().next();
                key = keys::load_secret_key(&s.key, passphrase.as_ref().map(|s| s.as_str()));
            }
            let key = key.map_err(|e| format!("Could not load private key: {e}"))?;
            drop(passphrase);
            prompts.lifecycle.ensure_active()?;
            let hash = handle
                .best_supported_rsa_hash()
                .await
                .map_err(|e| e.to_string())?
                .flatten();
            prompts.lifecycle.ensure_active()?;
            let result = handle
                .authenticate_publickey(
                    user.clone(),
                    PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                )
                .await
                .map_err(|e| e.to_string())?;
            if result.success() {
                return Ok(user);
            }
            if let russh::client::AuthResult::Failure {
                remaining_methods, ..
            } = result
            {
                remaining = remaining_methods;
            }
        } else {
            let agent_endpoint = match s.agent_mode {
                crate::model::AgentMode::Disabled => None,
                crate::model::AgentMode::Owned => agent_endpoint.cloned(),
                crate::model::AgentMode::Pageant => {
                    Some(crate::credentials::agent::AgentEndpoint::Pageant)
                }
                crate::model::AgentMode::System => agent_endpoint
                    .cloned()
                    .or_else(crate::credentials::agent::AgentEndpoint::system),
            };
            if let Some(agent_endpoint) = agent_endpoint {
                if let Ok(mut agent) = agent_endpoint.connect().await {
                    if let Ok(identities) = agent.request_identities().await {
                        for identity in identities.into_iter().take(32) {
                            let hash = handle
                                .best_supported_rsa_hash()
                                .await
                                .map_err(|e| e.to_string())?
                                .flatten();
                            prompts.lifecycle.ensure_active()?;
                            if let Ok(result) = handle
                                .authenticate_publickey_with(
                                    user.clone(),
                                    identity.public_key().into_owned(),
                                    hash,
                                    &mut agent,
                                )
                                .await
                            {
                                let partial = matches!(
                                    &result,
                                    client::AuthResult::Failure {
                                        partial_success: true,
                                        ..
                                    }
                                );
                                if update_methods(result, &mut remaining) {
                                    return Ok(user);
                                }
                                if partial {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    let mut password_attempts = 0;
    let mut interactive_attempts = 0;
    for _ in 0..16 {
        prompts.lifecycle.ensure_active()?;
        if remaining.contains(&MethodKind::KeyboardInteractive) && interactive_attempts < 3 {
            interactive_attempts += 1;
            let mut response = handle
                .authenticate_keyboard_interactive_start(user.clone(), None::<String>)
                .await
                .map_err(|e| e.to_string())?;
            for round in 0..16 {
                match response {
                    client::KeyboardInteractiveAuthResponse::Success => return Ok(user),
                    client::KeyboardInteractiveAuthResponse::InfoRequest {
                        name,
                        instructions,
                        prompts: fields,
                    } => {
                        let fields = fields
                            .into_iter()
                            .map(|p| Prompt {
                                label: p.prompt,
                                echo: p.echo,
                            })
                            .collect::<Vec<_>>();
                        if fields.len() > 32 {
                            return Err("Server requested too many authentication fields".into());
                        }
                        let count = fields.len();
                        let values = prompts
                            .connected_values(
                                handle,
                                format!("{endpoint} · Sign in as {user}\n{name}\n{instructions}"),
                                fields,
                                None,
                            )
                            .await?;
                        if values.len() != count {
                            return Err(
                                "Authentication response does not match the server challenge"
                                    .into(),
                            );
                        }
                        if round == 15 {
                            return Err(
                                "SSH authentication exceeded the challenge round limit".into()
                            );
                        }
                        prompts.lifecycle.ensure_active()?;
                        response = handle
                            .authenticate_keyboard_interactive_respond(
                                values.iter().map(|v| v.to_string()).collect(),
                            )
                            .await
                            .map_err(|e| e.to_string())?;
                    }
                    client::KeyboardInteractiveAuthResponse::Failure {
                        remaining_methods, ..
                    } => {
                        remaining = remaining_methods;
                        break;
                    }
                }
            }
        } else if remaining.contains(&MethodKind::Password) && password_attempts < 3 {
            password_attempts += 1;
            let values = prompts
                .connected_values(
                    handle,
                    format!("{endpoint} · Sign in as {user}"),
                    vec![Prompt {
                        label: "Password".into(),
                        echo: false,
                    }],
                    prompts.credential(&user, crate::credentials::SshSecretKind::Password),
                )
                .await?;
            let password = values.first().ok_or("Password response missing")?;
            prompts.lifecycle.ensure_active()?;
            let result = handle
                .authenticate_password(user.clone(), password.as_str())
                .await
                .map_err(|e| e.to_string())?;
            if update_methods(result, &mut remaining) {
                return Ok(user);
            }
        } else {
            break;
        }
    }
    Err("SSH authentication failed. Check the username, key, agent or server authentication policy.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn agent_factor_preserves_new_authentication_methods() {
        let mut remaining = russh::MethodSet::from(&[MethodKind::PublicKey][..]);
        let response = client::AuthResult::Failure {
            remaining_methods: russh::MethodSet::from(&[MethodKind::KeyboardInteractive][..]),
            partial_success: true,
        };
        assert!(!update_methods(response, &mut remaining));
        assert!(remaining.contains(&MethodKind::KeyboardInteractive));
        assert!(!remaining.contains(&MethodKind::PublicKey));
    }
}

fn key_digest(path: &str) -> Option<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 8 * 1024 * 1024 {
        return None;
    }
    Some(
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

pub fn gateway_reference_key(host: &str, port: u16, user: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(serde_json::to_vec(&(host.to_ascii_lowercase(), port, user)).unwrap())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
