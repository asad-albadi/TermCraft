use ironrdp_displaycontrol::{client::DisplayControlClient, pdu::MonitorLayoutEntry};
use ironrdp_dvc::DrdynvcClient;

use ironrdp_egfx::{
    client::{GraphicsPipelineClient, GraphicsPipelineHandler},
    pdu::{CapabilitiesV8Flags, CapabilitySet},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
#[derive(Default, Clone)]
pub struct GraphicsReset(Arc<AtomicBool>);
impl GraphicsReset {
    pub fn take(&self) -> bool {
        self.0.swap(false, Ordering::Relaxed)
    }
}
impl GraphicsPipelineHandler for GraphicsReset {
    fn capabilities(&self) -> Vec<CapabilitySet> {
        vec![CapabilitySet::V8 {
            flags: CapabilitiesV8Flags::SMALL_CACHE,
        }]
    }
    fn on_reset_graphics(&mut self, _width: u32, _height: u32) {
        self.0.store(true, Ordering::Relaxed);
    }
}
pub fn channel(reset: GraphicsReset) -> DrdynvcClient {
    DrdynvcClient::new()
        .with_dynamic_channel_factory(ironrdp_displaycontrol::CHANNEL_NAME, || {
            DisplayControlClient::new(|_| Ok(Vec::new()))
        })
        .with_dynamic_channel_factory(ironrdp_egfx::CHANNEL_NAME, move || {
            GraphicsPipelineClient::new(Box::new(reset.clone()), None)
        })
}

#[derive(Default)]
pub struct PendingResize(Option<(u32, u32)>);

impl PendingResize {
    pub fn request(&mut self, width: u16, height: u16) -> Result<(), String> {
        crate::wire::dimensions(width, height)?;
        self.0 = Some(MonitorLayoutEntry::adjust_display_size(
            width.into(),
            height.into(),
        ));
        Ok(())
    }

    // IronRDP returns None until the server has sent display-control capabilities.
    pub fn poll<T>(&mut self, encode: impl FnOnce(u32, u32) -> Option<T>) -> Option<T> {
        let (width, height) = self.0?;
        let result = encode(width, height)?;
        self.0 = None;
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_channel_reopens_and_waits_for_fresh_capabilities() {
        use ironrdp_displaycontrol::pdu::{DisplayControlCapabilities, DisplayControlPdu};
        use ironrdp_dvc::pdu::{CreateRequestPdu, DataPdu, DrdynvcDataPdu, DrdynvcServerPdu};
        use ironrdp_svc::SvcProcessor;

        let mut client = channel(Default::default());
        for id in [1, 2] {
            let open = DrdynvcServerPdu::Create(CreateRequestPdu::new(
                id,
                ironrdp_displaycontrol::CHANNEL_NAME.into(),
            ));
            client
                .process(&ironrdp_core::encode_vec(&open).unwrap())
                .unwrap();
            assert!(
                !client
                    .get_dvc::<DisplayControlClient>()
                    .unwrap()
                    .processor()
                    .ready()
            );
            let caps =
                DisplayControlPdu::Caps(DisplayControlCapabilities::new(1, 2048, 2048).unwrap());
            let data = DrdynvcServerPdu::Data(DrdynvcDataPdu::Data(DataPdu::new(
                id,
                ironrdp_core::encode_vec(&caps).unwrap(),
            )));
            client
                .process(&ironrdp_core::encode_vec(&data).unwrap())
                .unwrap();
            assert!(
                client
                    .get_dvc::<DisplayControlClient>()
                    .unwrap()
                    .processor()
                    .ready()
            );
            assert!(client.close_channel(id).is_some());
        }
    }

    #[tokio::test]
    async fn resize_during_certificate_prompt_preserves_prompt_identity() {
        use crate::wire::{Input, TrustDecision};
        let (tx, mut rx) = tokio::sync::mpsc::channel(3);
        let mut resize = PendingResize::default();
        tx.send(Input::Resize {
            width: 1000,
            height: 600,
        })
        .await
        .unwrap();
        tx.send(Input::Trust {
            id: 7,
            decision: TrustDecision::Reject,
        })
        .await
        .unwrap();
        assert!(matches!(
            crate::answer(&mut rx, 7, true, &mut resize).await,
            Ok(Input::Trust {
                id: 7,
                decision: TrustDecision::Reject
            })
        ));
        assert_eq!(resize.poll(|w, h| Some((w, h))), Some((1000, 600)));
    }

    #[test]
    fn only_latest_pane_size_is_sent_when_capabilities_arrive() {
        let mut resize = PendingResize::default();
        resize.request(1200, 900).unwrap();
        assert_eq!(resize.poll(|_, _| None::<()>), None);
        resize.request(999, 180).unwrap();
        assert_eq!(resize.poll(|w, h| Some((w, h))), Some((998, 200)));
        assert!(
            resize
                .poll::<()>(|_, _| panic!("duplicate resize"))
                .is_none()
        );
    }

    #[test]
    fn invalid_request_preserves_previous_bounded_size() {
        let mut resize = PendingResize::default();
        resize.request(800, 600).unwrap();
        assert!(resize.request(8192, 8192).is_err());
        assert_eq!(resize.poll(|w, h| Some((w, h))), Some((800, 600)));
    }
}
