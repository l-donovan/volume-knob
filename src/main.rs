#![no_std]
#![no_main]

mod button;
mod hid;
mod hid_descriptor;
mod led;

use bleps::{
    Addr, Ble, HciConnector,
    ad_structure::{
        AD_FLAG_LE_LIMITED_DISCOVERABLE, AdStructure, BR_EDR_NOT_SUPPORTED, create_advertising_data,
    },
    att::Uuid,
    attribute_server::{AttributeServer, WorkResult},
    gatt,
};
use button::Button;
use core::{cell::RefCell, cmp::min};
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    gpio::{Input, Pull},
    main,
    rmt::Rmt,
    rng::Trng,
    time::{self, RateExtU32},
    timer::timg::TimerGroup,
};
use esp_hal_smartled::{SmartLedsAdapter, smartLedBuffer};
use esp_wifi::ble::controller::BleConnector;
use hid::{MediaKeys, SendsKeypresses};
use hid_descriptor::{HID_REPORT, HID_REPORT_INPUT1_ID};
use led::{Colorable, hue};
use log::info;

// 0x02, vid (u16), pid (u16), version (u16)
const DEVICE_INFO: &[u8] = &[0x02, 0x37, 0x13, 0x37, 0x13, 0x37, 0x13];
const DEVICE_MANUFACTURER: &[u8] = b"Luke Enterprises";
// format (u8), exponent (i8), unit (u16), namespace (u8), description (u16)
const BATTERY_FORMAT: &[u8] = &[0x04, 0x00, 0x27, 0xad, 0x01, 0x00, 0x00];
// report ID (u8), input 0x01/output 0x02/feature 0x03 (u8)
const REPORT_REFERENCE: &[u8] = &[HID_REPORT_INPUT1_ID, 0x01];
// HID spec version 0x0101 (u16), country (u8), flags (u8)
const HID_INFO: &[u8] = &[0x01, 0x01, 0x00, 0x02];

fn write(offset: usize, dst: &mut [u8], src: &[u8]) -> usize {
    let bytes_to_read = min(dst.len(), src.len() - offset);
    dst[..bytes_to_read].copy_from_slice(&src[offset..offset + bytes_to_read]);
    bytes_to_read
}

#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();

    info!("Starting up");

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    let led_pin = peripherals.GPIO8;
    let freq = 80.MHz();
    let rmt = Rmt::new(peripherals.RMT, freq).unwrap();
    let mut trng = Trng::new(peripherals.RNG, peripherals.ADC1);

    // We use one of the RMT channels to instantiate a `SmartLedsAdapter` which can
    // be used directly with all `smart_led` implementations
    let rmt_buffer = smartLedBuffer!(1);
    let mut led = SmartLedsAdapter::new(rmt.channel0, led_pin, rmt_buffer);

    // Allocate 72 kB to the heap
    esp_alloc::heap_allocator!(72 * 1024);
    info!("Allocated heap");

    let timer_group_0 = TimerGroup::new(peripherals.TIMG0);
    info!("Created timer group");

    // Initialize the WiFi system
    let wifi_controller = esp_wifi::init(timer_group_0.timer0, trng.rng, peripherals.RADIO_CLK)
        .inspect_err(|_| led.set_hue(hue::RED))
        .unwrap();

    info!("Initialized WiFi controller");

    let mut play_pause_button = Button::new(Input::new(peripherals.GPIO9, Pull::Down));
    let mut bluetooth = peripherals.BT;

    let now = || time::now().duration_since_epoch().to_millis();

    let mut ltk = None;

    loop {
        let connector = BleConnector::new(&wifi_controller, &mut bluetooth);
        let hci = HciConnector::new(connector, now);
        let mut ble = Ble::new(&hci);

        ble.init().inspect_err(|_| led.set_hue(hue::RED)).unwrap();

        let local_addr = Addr::from_le_bytes(false, ble.cmd_read_br_addr().unwrap());

        ble.cmd_set_le_advertising_parameters()
            .inspect_err(|_| led.set_hue(hue::RED))
            .unwrap();

        let advertising_data = create_advertising_data(&[
            AdStructure::Flags(AD_FLAG_LE_LIMITED_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            // See https://bitbucket.org/bluetooth-SIG/public/src/main/assigned_numbers/uuids/service_uuids.yaml
            AdStructure::ServiceUuids16(&[
                Uuid::Uuid16(0x1812), // HID
                Uuid::Uuid16(0x180f), // Battery
                Uuid::Uuid16(0x180a), // Device information
            ]),
            AdStructure::CompleteLocalName("vKnob"),
            AdStructure::ManufacturerSpecificData {
                company_identifier: 0x1337,
                payload: &[],
            },
            AdStructure::Unknown {
                ty: 0x19,            // Appearance
                data: &[0xc1, 0x03], // Keyboard
            },
        ])
        .inspect_err(|_| led.set_hue(hue::RED))
        .unwrap();

        ble.cmd_set_le_advertising_data(advertising_data)
            .inspect_err(|_| led.set_hue(hue::RED))
            .unwrap();

        ble.cmd_set_le_advertise_enable(true)
            .inspect_err(|_| led.set_hue(hue::RED))
            .unwrap();

        info!("Started advertising");
        led.set_hue(hue::GREEN);

        // Define our read/write closures
        let mut hid_info_read = |o: usize, d: &mut [u8]| write(o, d, &HID_INFO);
        let mut battery_format_read = |o: usize, d: &mut [u8]| write(o, d, &BATTERY_FORMAT);
        let mut report_reference_read = |o: usize, d: &mut [u8]| write(o, d, &REPORT_REFERENCE);
        let mut report_map_read = |o: usize, d: &mut [u8]| write(o, d, &HID_REPORT);
        let mut battery_level_read = |o: usize, d: &mut [u8]| write(o, d, &[0x50]);
        let mut input_report_read = |o: usize, d: &mut [u8]| write(o, d, &[0b00000001]);

        let mut hid_control_write = |offset: usize, data: &[u8]| {
            // TODO
        };

        // Protocol mode might actually be one measly Byte
        let protocol_mode = RefCell::new([0u8; 128]);
        let protocol_mode_len = RefCell::new(1usize);

        let mut protocol_mode_read = |offset: usize, data: &mut [u8]| {
            info!("Protocol mode read called with offset {offset}");
            let mode = protocol_mode.borrow();
            let len = *protocol_mode_len.borrow();
            let bytes_to_read = min(data.len(), len - offset);
            data[..bytes_to_read].copy_from_slice(&mode[offset..offset + bytes_to_read]);
            bytes_to_read
        };

        let mut protocol_mode_write = |offset: usize, data: &[u8]| {
            info!("Protocol mode write was called with offset {offset} and data {data:#?}");
            let mut mode = protocol_mode.borrow_mut();
            let mut len = protocol_mode_len.borrow_mut();
            *len = data.len();
            mode[..*len].copy_from_slice(&data[offset..offset + *len]);
        };

        gatt!([
            service {
                uuid: "180a", // Device information
                characteristics: [
                    characteristic {
                        name: "manufacturer_characteristic",
                        uuid: "2a29",
                        value: DEVICE_MANUFACTURER,
                    },
                    characteristic {
                        name: "pnp_characteristic",
                        uuid: "2a50",
                        value: DEVICE_INFO,
                    }
                ],
            },
            service {
                uuid: "180f", // Battery
                characteristics: [characteristic {
                    name: "battery_level_characteristic",
                    uuid: "2a19",
                    notify: true, // This automatically creates a 0x2902 descriptor
                    read: battery_level_read,
                    descriptors: [descriptor {
                        uuid: "2904", // Presentation format
                        read: battery_format_read,
                    }]
                }],
            },
            service {
                uuid: "1812", // HID
                characteristics: [
                    // These characteristics are required for all HID devices
                    characteristic {
                        name: "hid_info_characteristic",
                        uuid: "2a4a",
                        read: hid_info_read,
                    },
                    characteristic {
                        name: "hid_control_characteristic",
                        uuid: "2a4c",
                        write: hid_control_write,
                    },
                    characteristic {
                        name: "report_map_characteristic",
                        uuid: "2a4b",
                        read: report_map_read,
                    },
                    // This characteristic does... something
                    characteristic {
                        name: "protocol_mode_characteristic",
                        uuid: "2a4e",
                        read: protocol_mode_read,
                        write: protocol_mode_write,
                    },
                    // This characteristic is responsible for actually sending the data to the host
                    characteristic {
                        name: "input_report_characteristic",
                        uuid: "2a4d",
                        notify: true, // This automatically creates a 0x2902 descriptor
                        read: input_report_read,
                        descriptors: [descriptor {
                            uuid: "2908", // Report reference
                            read: report_reference_read,
                        }],
                    },
                ],
            },
        ]);

        let mut srv = AttributeServer::new_with_ltk(
            &mut ble,
            &mut gatt_attributes,
            local_addr,
            ltk,
            &mut trng,
        );

        let mut pin_callback = |pin: u32| {
            info!("PIN is {pin}");
        };

        srv.set_pin_callback(Some(&mut pin_callback));

        loop {
            if play_pause_button.when_pressed(|| {
                srv.send_keypress(
                    input_report_characteristic_notify_enable_handle,
                    input_report_characteristic_handle,
                    MediaKeys::PlayPause,
                )
            }) {
                break;
            }

            match srv.do_work_with_notification(None) {
                Ok(WorkResult::GotDisconnected) => {
                    break;
                }
                Err(err) => {
                    info!("{:?}", err);
                }
                _ => {}
            }

            ltk = srv.get_ltk();
        }
    }
}
