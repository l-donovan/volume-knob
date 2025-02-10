#![no_std]
#![no_main]

mod hid_descriptor;
mod led;

use bleps::{
    Addr, Ble, HciConnector,
    ad_structure::{
        AdStructure, BR_EDR_NOT_SUPPORTED, LE_GENERAL_DISCOVERABLE, create_advertising_data,
    },
    att::Uuid,
    attribute_server::{AttributeServer, NotificationData, WorkResult},
    gatt,
};
use esp_backtrace as _;
use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{Input, Pull},
    main,
    rmt::Rmt,
    rng::Trng,
    time::{self, RateExtU32},
    timer::timg::TimerGroup,
};
use esp_hal_smartled::{SmartLedsAdapter, smartLedBuffer};
use esp_wifi::ble::controller::BleConnector;
use hid_descriptor::{HID_REPORT, HID_REPORT_INPUT1_ID, HID_REPORT_INPUT1_SIZE, HID_REPORT_SIZE};
use led::{Colorable, hue};
use log::{info, warn};

// 0x02, vid (u16), pid (u16), version (u16)
const DEVICE_INFO: &[u8] = &[0x02, 0x37, 0x13, 0x37, 0x13, 0x37, 0x13];
const DEVICE_MANUFACTURER: &[u8] = b"Luke Enterprises";
// format (u8), exponent (i8), unit (u16), namespace (u8), description (u16)
const BATTERY_FORMAT: &[u8] = &[0x04, 0x00, 0x27, 0xad, 0x01, 0x00, 0x00];

// report ID (u8), input 0x01/output 0x02/feature 0x03 (u8)
const REPORT_REFERENCE: &[u8] = &[HID_REPORT_INPUT1_ID, 0x01];

#[main]
fn main() -> ! {
    esp_println::logger::init_logger_from_env();

    info!("Starting up");

    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    let led_pin = peripherals.GPIO8;
    let freq = 80.MHz();
    let rmt = Rmt::new(peripherals.RMT, freq).unwrap();
    let mut trng = Trng::new(peripherals.RNG, peripherals.ADC1);
    let delay = Delay::new();

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
        .inspect_err(|e| {
            led.set_hue(hue::RED);
            panic!("Error initializing WiFi controller: {:#?}", e);
        })
        .unwrap();

    info!("Successfully initialized WiFi controller");

    let button = Input::new(peripherals.GPIO9, Pull::Down);
    let mut debounce_cnt = 500;
    let mut bluetooth = peripherals.BT;

    let now = || time::now().duration_since_epoch().to_millis();

    let mut ltk = None;

    loop {
        let connector = BleConnector::new(&wifi_controller, &mut bluetooth);
        let hci = HciConnector::new(connector, now);
        let mut ble = Ble::new(&hci);

        ble.init()
            .inspect_err(|e| {
                led.set_hue(hue::RED);
                warn!("Failed to initialize BLE: {:#?}", e);
            })
            .unwrap();

        let local_addr = Addr::from_le_bytes(false, ble.cmd_read_br_addr().unwrap());

        ble.cmd_set_le_advertising_parameters()
            .inspect_err(|e| {
                led.set_hue(hue::RED);
                warn!("Failed to set LE advertising parameters: {:#?}", e);
            })
            .unwrap();

        let advertising_data = create_advertising_data(&[
            AdStructure::Flags(LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED),
            // HID and Device Information
            // per https://bitbucket.org/bluetooth-SIG/public/src/main/assigned_numbers/uuids/service_uuids.yaml
            AdStructure::ServiceUuids16(&[
                // Uuid::Uuid16(0x180a),
                // Uuid::Uuid16(0x180f),
                Uuid::Uuid16(0x1812),
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
        .inspect_err(|e| {
            led.set_hue(hue::RED);
            warn!("Failed to create advertising data: {:#?}", e);
        })
        .unwrap();

        ble.cmd_set_le_advertising_data(advertising_data)
            .inspect_err(|e| {
                led.set_hue(hue::RED);
                warn!("Failed to set advertising data: {:#?}", e);
            })
            .unwrap();

        ble.cmd_set_le_advertise_enable(true)
            .inspect_err(|e| {
                led.set_hue(hue::RED);
                warn!("Failed to enable advertising: {:#?}", e);
            })
            .unwrap();

        info!("Started advertising");
        led.set_hue(hue::GREEN);

        // HID spec version 0x0111 (u16), country (u8), flags (u8)
        let mut hid_info_read = |offset: usize, data: &mut [u8]| {
            data[..4].copy_from_slice(&[0x11, 0x01, 0x00, 0x02]);
            4
        };

        let mut report_map_read = |offset: usize, data: &mut [u8]| {
            data[..HID_REPORT_SIZE].copy_from_slice(&HID_REPORT);
            HID_REPORT_SIZE
        };

        let mut hid_control_write = |offset: usize, data: &[u8]| {};

        let mut protocol_mode_read = |offset: usize, data: &mut [u8]| {
            data[0] = 0x01;
            1
        };

        let mut protocol_mode_write = |offset: usize, data: &[u8]| {};

        let mut battery_level_read = |offset: usize, data: &mut [u8]| {
            data[0] = 0x50;
            1
        };

        let mut input_report_read = |offset: usize, data: &mut [u8]| {
            data[..HID_REPORT_INPUT1_SIZE].copy_from_slice(&[0xe9]);
            HID_REPORT_INPUT1_SIZE
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
                    // // I can't explain why this breaks things, but it does.
                    // descriptors: [descriptor {
                    //     uuid: "2904", // Characteristic presentation format
                    //     value: BATTERY_FORMAT
                    // }]
                }],
            },
            service {
                uuid: "1812", // HID
                characteristics: [
                    characteristic {
                        name: "hid_info_characteristic",
                        uuid: "2a4a",
                        read: hid_info_read,
                    },
                    characteristic {
                        name: "report_map_characteristic",
                        uuid: "2a4b",
                        read: report_map_read,
                    },
                    characteristic {
                        name: "hid_control_characteristic",
                        uuid: "2a4c",
                        write: hid_control_write,
                    },
                    characteristic {
                        name: "input_report_characteristic",
                        uuid: "2a4d",
                        notify: true, // This automatically creates a 0x2902 descriptor
                        read: input_report_read,
                        descriptors: [descriptor {
                            uuid: "2908", // Report reference
                            value: REPORT_REFERENCE,
                        }],
                    },
                    characteristic {
                        name: "protocol_mode_characteristic",
                        uuid: "2a4e",
                        read: protocol_mode_read,
                        write: protocol_mode_write,
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
            let mut notification = None;

            if button.is_low() && debounce_cnt > 0 {
                debounce_cnt -= 1;

                if debounce_cnt == 0 {
                    let mut cccd = [0u8; 1];

                    if let Some(1) = srv.get_characteristic_value(
                        input_report_characteristic_notify_enable_handle,
                        0,
                        &mut cccd,
                    ) {
                        info!("{cccd:#?}");
                        // if notifications enabled
                        if cccd[0] == 1 {
                            led.set_hue(hue::RED);
                            delay.delay_millis(50);
                            led.set_hue(hue::GREEN);
                            delay.delay_millis(50);

                            notification = Some(NotificationData::new(
                                input_report_characteristic_handle,
                                &[0b00000001],
                            ));
                        }
                    }
                }
            }

            if button.is_high() {
                debounce_cnt = 500;
            }

            if let Some(ref not) = notification {
                info!("Notification: {not:#?}");
            }

            match srv.do_work_with_notification(notification) {
                Ok(res) => {
                    if let WorkResult::GotDisconnected = res {
                        break;
                    }
                }
                Err(err) => {
                    info!("{:?}", err);
                }
            }

            ltk = srv.get_ltk();
        }
    }
}
