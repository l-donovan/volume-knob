use esp_hal::gpio::Input;

pub struct Button<'a> {
    input: Input<'a>,
    debounce_count: u16,
}

impl<'a> Button<'a> {
    pub fn new(input: Input<'a>) -> Self {
        Self {
            input,
            debounce_count: 500,
        }
    }

    pub fn is_low(&mut self) -> bool {
        self.input.is_low()
    }

    pub fn is_high(&mut self) -> bool {
        self.input.is_high()
    }

    pub fn when_pressed<F>(&mut self, mut callback: F) -> bool
    where
        F: FnMut() -> bool,
    {
        if self.is_low() && self.debounce_count > 0 {
            self.debounce_count -= 1;

            if self.debounce_count == 0 {
                return callback();
            }
        } else if self.is_high() {
            self.debounce_count = 500;
        }

        false
    }
}
