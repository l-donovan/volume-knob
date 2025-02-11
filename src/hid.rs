use bitflags::bitflags;
use bleps::attribute_server::{AttributeServer, NotificationData, WorkResult};
use esp_hal::rng::Trng;
use log::info;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct MediaKeys: u8 {
        const Clear     = 0b00000000;
        const VolUp     = 0b00000001;
        const VolDown   = 0b00000010;
        const Mute      = 0b00000100;
        const PlayPause = 0b00001000;
        const Stop      = 0b00010000;
        const NextTrack = 0b00100000;
        const PrevTrack = 0b01000000;
    }
}

pub trait SendsKeypresses {
    fn press(&mut self, handle: u16, keys: MediaKeys) -> bool;
    fn clear(&mut self, handle: u16) -> bool;
}

impl<'a, 'b> SendsKeypresses for AttributeServer<'a, Trng<'b>> {
    fn press(&mut self, handle: u16, keys: MediaKeys) -> bool {
        match self.do_work_with_notification(Some(NotificationData::new(handle, &[keys.bits()]))) {
            Ok(res) => {
                if let WorkResult::GotDisconnected = res {
                    true
                } else {
                    false
                }
            }
            Err(err) => {
                info!("{:?}", err);
                false
            }
        }
    }

    fn clear(&mut self, handle: u16) -> bool {
        self.press(handle, MediaKeys::Clear)
    }
}
