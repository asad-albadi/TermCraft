use crate::display::{GraphicsReset, channel};
use ironrdp_dvc::DrdynvcClient;
use ironrdp_dvc::pdu::{
    CreateRequestPdu, DataPdu, DrdynvcClientPdu, DrdynvcDataPdu, DrdynvcServerPdu,
};
use ironrdp_egfx::client::GraphicsPipelineClient;
use ironrdp_egfx::pdu::*;
use ironrdp_graphics::{
    image_processing::PixelFormat as ImageFormat,
    rdp6::{BitmapStreamEncoder, RgbChannels},
};
use ironrdp_pdu::geometry::ExclusiveRectangle;
use ironrdp_session::{composite_graphics_pipeline, image::DecodedImage};
use ironrdp_svc::{SvcMessage, SvcProcessor};

fn responses(messages: Vec<SvcMessage>) -> Vec<GfxPdu> {
    messages
        .into_iter()
        .filter_map(|m| {
            let bytes = m.encode_unframed_pdu().unwrap();
            match ironrdp_core::decode::<DrdynvcClientPdu>(&bytes).unwrap() {
                DrdynvcClientPdu::Data(DrdynvcDataPdu::Data(data)) => {
                    Some(ironrdp_core::decode(data.data()).unwrap())
                }
                _ => None,
            }
        })
        .collect()
}
fn send(client: &mut DrdynvcClient, id: u32, pdu: GfxPdu) -> Vec<GfxPdu> {
    let encoded = ironrdp_core::encode_vec(&pdu).unwrap();
    let payload = ironrdp_graphics::zgfx::wrap_uncompressed(&encoded);
    let data = DrdynvcServerPdu::Data(DrdynvcDataPdu::Data(DataPdu::new(id, payload)));
    responses(
        client
            .process(&ironrdp_core::encode_vec(&data).unwrap())
            .unwrap(),
    )
}

#[test]
fn reopened_graphics_channel_composites_planar_pixels_and_acknowledges_each_frame() {
    let reset = GraphicsReset::default();
    let mut client = channel(reset.clone());
    let mut image = DecodedImage::new(ImageFormat::RgbA32, 2, 2);
    for id in [4, 7] {
        let create =
            DrdynvcServerPdu::Create(CreateRequestPdu::new(id, ironrdp_egfx::CHANNEL_NAME.into()));
        let reply = responses(
            client
                .process(&ironrdp_core::encode_vec(&create).unwrap())
                .unwrap(),
        );
        let [GfxPdu::CapabilitiesAdvertise(caps)] = reply.as_slice() else {
            panic!("missing graphics capabilities")
        };
        let allowed = CapabilitySet::V8 {
            flags: CapabilitiesV8Flags::SMALL_CACHE,
        };
        assert_eq!(caps.0, vec![(&allowed).into()]);
        send(
            &mut client,
            id,
            GfxPdu::CapabilitiesConfirm(CapabilitiesConfirmPdu::from_typed(&allowed)),
        );
        send(
            &mut client,
            id,
            GfxPdu::ResetGraphics(ResetGraphicsPdu {
                width: 20,
                height: 12,
                monitors: vec![],
            }),
        );
        assert!(reset.take());
        let drain = |client: &mut DrdynvcClient, image: &mut DecodedImage| {
            let mut gfx = client.get_dvc_mut::<GraphicsPipelineClient>().unwrap();
            composite_graphics_pipeline(gfx.processor_mut(), image).unwrap()
        };
        assert!(drain(&mut client, &mut image).reset);
        assert_eq!((image.width(), image.height()), (20, 12));
        send(
            &mut client,
            id,
            GfxPdu::CreateSurface(CreateSurfacePdu {
                surface_id: 1,
                width: 8,
                height: 4,
                pixel_format: PixelFormat::XRgb,
            }),
        );
        send(
            &mut client,
            id,
            GfxPdu::MapSurfaceToOutput(MapSurfaceToOutputPdu {
                surface_id: 1,
                output_origin_x: 5,
                output_origin_y: 3,
            }),
        );
        send(
            &mut client,
            id,
            GfxPdu::StartFrame(StartFramePdu {
                frame_id: 27,
                timestamp: Timestamp {
                    hours: 0,
                    minutes: 0,
                    seconds: 0,
                    milliseconds: 1,
                },
            }),
        );
        let color = [43, 121, 211];
        let mut encoded = vec![0; 256];
        let n = BitmapStreamEncoder::new(8, 4)
            .encode_bitmap::<RgbChannels>(&color.repeat(32), &mut encoded, true)
            .unwrap();
        encoded.truncate(n);
        assert!(n < 32 * 3);
        send(
            &mut client,
            id,
            GfxPdu::WireToSurface1(WireToSurface1Pdu {
                surface_id: 1,
                codec_id: Codec1Type::Planar,
                pixel_format: PixelFormat::XRgb,
                destination_rectangle: ExclusiveRectangle {
                    left: 0,
                    top: 0,
                    right: 8,
                    bottom: 4,
                },
                bitmap_data: encoded,
            }),
        );
        let ack = send(
            &mut client,
            id,
            GfxPdu::EndFrame(EndFramePdu { frame_id: 27 }),
        );
        assert!(ack.iter().any(|p| matches!(p,GfxPdu::FrameAcknowledge(a) if a.frame_id==27 && a.total_frames_decoded==1)));
        let rect = drain(&mut client, &mut image)
            .region
            .expect("missing composed pixels");
        assert_eq!(
            (rect.left, rect.top, rect.right, rect.bottom),
            (5, 3, 12, 6)
        );
        let mut damage = crate::output::Damage::Clean;
        damage.update(rect, image.width(), image.height());
        let packet = damage.packet(&image, 2).unwrap();
        let bytes = packet.data;
        assert_eq!(packet.kind, 3);
        assert_eq!(&bytes[12..20], &[0, 5, 0, 3, 0, 8, 0, 4]);
        assert_eq!(&bytes[20..], [43, 121, 211, 255].repeat(32));
        assert!(client.close_channel(id).is_some());
    }
}
