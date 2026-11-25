//! Portable SSH settings; credentials are opaque references only.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProxyKind {
    #[default]
    HttpConnect,
    Socks5,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProxySettings {
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub credential_ref: Option<String>,
    pub authenticate: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct SshDefaults {
    pub keepalive_secs: u32,
    pub connect_timeout_secs: u32,
}
impl Default for SshDefaults {
    fn default() -> Self {
        Self {
            keepalive_secs: 60,
            connect_timeout_secs: 10,
        }
    }
}
impl SshDefaults {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct SshOptions {
    pub keepalive_secs: Option<u32>,
    pub keepalive_disabled: bool,
    pub connect_timeout_secs: Option<u32>,
    pub proxy: Option<ProxySettings>,
    pub config_file: Option<PathBuf>,
    pub prefer_config: bool,
    pub jump_limit: u8,
}
impl Default for SshOptions {
    fn default() -> Self {
        Self {
            keepalive_secs: None,
            keepalive_disabled: false,
            connect_timeout_secs: None,
            proxy: None,
            config_file: None,
            prefer_config: false,
            jump_limit: 4,
        }
    }
}
impl SshOptions {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
    pub fn clear_credentials(&mut self) {
        if let Some(proxy) = &mut self.proxy {
            proxy.credential_ref = None;
        }
    }
    pub fn credential_context(&self) -> Option<serde_json::Value> {
        if self.proxy.is_none() && self.config_file.is_none() && !self.prefer_config {
            return None;
        }
        let mut proxy = self.proxy.clone();
        if let Some(proxy) = &mut proxy {
            proxy.credential_ref = None;
        }
        Some(serde_json::to_value((&proxy, &self.config_file, self.prefer_config)).unwrap())
    }
}
