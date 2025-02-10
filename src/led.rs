use esp_hal::rmt::TxChannel;
use esp_hal_smartled::SmartLedsAdapter;
use smart_leds::{
    SmartLedsWrite, brightness, gamma,
    hsv::{Hsv, hsv2rgb},
};

pub mod hue {
    pub const RED: u8 = 0;
    pub const YELLOW: u8 = 35;
    pub const GREEN: u8 = 85;
}

pub trait Colorable {
    fn set_hue(&mut self, hue: u8);
}

impl<T: TxChannel, const N: usize> Colorable for SmartLedsAdapter<T, N> {
    fn set_hue(&mut self, hue: u8) {
        let color = Hsv {
            hue,
            sat: 0xff,
            val: 0xff,
        };

        let data = [hsv2rgb(color)];
        self.write(brightness(gamma(data.iter().cloned()), 10))
            .unwrap();
    }
}
