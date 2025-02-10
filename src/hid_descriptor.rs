pub static HID_REPORT_SIZE: usize = 37;
pub static HID_REPORT: [u8; HID_REPORT_SIZE] = [
    0x05, 0x0C,    // UsagePage(Consumer[0x000C])
    0x09, 0x01,    // UsageId(Consumer Control[0x0001])
    0xA1, 0x01,    // Collection(Application)
    0x85, 0x01,    //     ReportId(1)
    0x09, 0xE9,    //     UsageId(Volume Increment[0x00E9])
    0x09, 0xEA,    //     UsageId(Volume Decrement[0x00EA])
    0x09, 0xE2,    //     UsageId(Mute[0x00E2])
    0x09, 0xCD,    //     UsageId(Play/Pause[0x00CD])
    0x09, 0xB7,    //     UsageId(Stop[0x00B7])
    0x09, 0xB5,    //     UsageId(Scan Next Track[0x00B5])
    0x09, 0xB6,    //     UsageId(Scan Previous Track[0x00B6])
    0x15, 0x00,    //     LogicalMinimum(0)
    0x25, 0x01,    //     LogicalMaximum(1)
    0x95, 0x07,    //     ReportCount(7)
    0x75, 0x01,    //     ReportSize(1)
    0x81, 0x02,    //     Input(Data, Variable, Absolute, NoWrap, Linear, PreferredState, NoNullPosition, BitField)
    0x95, 0x01,    //     ReportCount(1)
    0x81, 0x03,    //     Input(Constant, Variable, Absolute, NoWrap, Linear, PreferredState, NoNullPosition, BitField)
    0xC0,          // EndCollection()
];

pub static HID_REPORT_INPUT1_ID: u8 = 1;
pub static HID_REPORT_INPUT1_SIZE: usize = 1;