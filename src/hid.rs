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
    fn send_keypress(&mut self, notify_handle: u16, data_handle: u16, keys: MediaKeys) -> bool;
}

impl<'a, 'b> SendsKeypresses for AttributeServer<'a, Trng<'b>> {
    fn send_keypress(&mut self, notify_handle: u16, data_handle: u16, keys: MediaKeys) -> bool {
        let mut cccd = [0u8; 1];

        if let Some(1) = self.get_characteristic_value(notify_handle, 0, &mut cccd) {
            // Should this be (cccd[0] & 0xb00000001) == 0 or something similar?
            if cccd[0] != 1 {
                return false;
            }

            // Press
            match self
                .do_work_with_notification(Some(NotificationData::new(data_handle, &[keys.bits()])))
            {
                Ok(WorkResult::GotDisconnected) => {
                    return true;
                }
                Err(err) => {
                    info!("{:?}", err);
                }
                _ => {}
            };

            // Clear
            match self.do_work_with_notification(Some(NotificationData::new(data_handle, &[0]))) {
                Ok(WorkResult::GotDisconnected) => {
                    return true;
                }
                Err(err) => {
                    info!("{:?}", err);
                }
                _ => {}
            };

            // NOTE: We can only clear all keypresses, because we don't maintain the current
            // keypress state, nor would it be appropriate for AtttributeServer to have
            // ownership.
        };

        return false;
    }
}
