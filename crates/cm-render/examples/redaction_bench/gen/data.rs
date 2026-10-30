//! Rule-based fake data. Every value is built from a published *format* and *checksum* rule over
//! random digits; nothing is read from, or linked to, a real record. The checksum code here is
//! independent of the detector (`crate::ocr::rules`): a bug in one does not hide a bug in the
//! other.

use super::rng::{Rng, ALNUM, B64, B64URL, HEX, LOWNUM, UPNUM, UPPER};

// ---------------------------------------------------------------------------------------------
// Checksums (also used by the tests as validators)
// ---------------------------------------------------------------------------------------------

fn dvals(s: &str) -> Vec<u32> {
    s.chars().filter_map(|c| c.to_digit(10)).collect()
}

/// Luhn check digit that makes `body + digit` valid.
pub fn luhn_check_digit(body: &str) -> u32 {
    let mut sum = 0;
    for (i, d) in dvals(body).iter().rev().enumerate() {
        let mut v = *d;
        if i % 2 == 0 {
            v *= 2;
            if v > 9 {
                v -= 9;
            }
        }
        sum += v;
    }
    (10 - sum % 10) % 10
}

pub fn luhn_valid(s: &str) -> bool {
    let d = dvals(s);
    if d.len() < 2 {
        return false;
    }
    let body: String = d[..d.len() - 1]
        .iter()
        .map(|v| char::from(b'0' + *v as u8))
        .collect();
    luhn_check_digit(&body) == d[d.len() - 1]
}

pub fn aba_valid(s: &str) -> bool {
    let d = dvals(s);
    if d.len() != 9 || s.len() != 9 {
        return false;
    }
    (3 * (d[0] + d[3] + d[6]) + 7 * (d[1] + d[4] + d[7]) + (d[2] + d[5] + d[8])).is_multiple_of(10)
}

pub fn npi_valid(s: &str) -> bool {
    s.len() == 10 && s.bytes().all(|b| b.is_ascii_digit()) && luhn_valid(&format!("80840{s}"))
}

#[cfg(test)]
pub fn dea_valid(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 9 || !b[0].is_ascii_uppercase() || !b[1].is_ascii_uppercase() {
        return false;
    }
    let d = dvals(&s[2..]);
    d.len() == 7 && (d[0] + d[2] + d[4] + 2 * (d[1] + d[3] + d[5])) % 10 == d[6]
}

fn mod97(s: &str) -> u32 {
    let mut rem = 0u32;
    for c in s.chars() {
        let v = match c {
            '0'..='9' => c.to_digit(10).unwrap_or(0),
            'A'..='Z' => c as u32 - 'A' as u32 + 10,
            _ => continue,
        };
        rem = if v >= 10 {
            (rem * 100 + v) % 97
        } else {
            (rem * 10 + v) % 97
        };
    }
    rem
}

#[cfg(test)]
pub fn iban_valid(s: &str) -> bool {
    let c: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if c.len() < 15 {
        return false;
    }
    let (head, tail) = c.split_at(4);
    mod97(&format!("{tail}{head}")) == 1
}

const VIN_ALPHA: &str = "ABCDEFGHJKLMNPRSTUVWXYZ";

fn vin_value(c: char) -> u32 {
    match c {
        '0'..='9' => c.to_digit(10).unwrap_or(0),
        'A' | 'J' => 1,
        'B' | 'K' | 'S' => 2,
        'C' | 'L' | 'T' => 3,
        'D' | 'M' | 'U' => 4,
        'E' | 'N' | 'V' => 5,
        'F' | 'W' => 6,
        'G' | 'P' | 'X' => 7,
        'H' | 'Y' => 8,
        'R' | 'Z' => 9,
        _ => 0,
    }
}

fn vin_check_char(v: &[char]) -> char {
    const W: [u32; 17] = [8, 7, 6, 5, 4, 3, 2, 10, 0, 9, 8, 7, 6, 5, 4, 3, 2];
    let sum: u32 = v.iter().zip(W).map(|(c, w)| vin_value(*c) * w).sum();
    match sum % 11 {
        10 => 'X',
        r => char::from(b'0' + r as u8),
    }
}

#[cfg(test)]
pub fn vin_valid(s: &str) -> bool {
    let v: Vec<char> = s.chars().collect();
    v.len() == 17
        && v.iter()
            .all(|c| c.is_ascii_digit() || VIN_ALPHA.contains(*c))
        && v[8] == vin_check_char(&v)
}

const MBI_ALPHA: &str = "ACDEFGHJKMNPQRTUVWXY";

#[cfg(test)]
pub fn mbi_valid(s: &str) -> bool {
    let c: Vec<char> = s.chars().filter(|c| *c != '-').collect();
    if c.len() != 11 {
        return false;
    }
    let al = |x: char| MBI_ALPHA.contains(x);
    let nu = |x: char| x.is_ascii_digit();
    let an = |x: char| al(x) || nu(x);
    ('1'..='9').contains(&c[0])
        && al(c[1])
        && an(c[2])
        && nu(c[3])
        && al(c[4])
        && an(c[5])
        && nu(c[6])
        && al(c[7])
        && al(c[8])
        && nu(c[9])
        && nu(c[10])
}

/// Campus prefixes the IRS assigns (ranges, not a list of real numbers).
pub fn ein_prefix_valid(p: u32) -> bool {
    matches!(p, 1..=6 | 10..=16 | 20..=27 | 30..=48 | 50..=68 | 71..=77 | 80..=88 | 90..=95 | 98 | 99)
}

pub fn ssn_issuable(area: u32, group: u32, serial: u32) -> bool {
    (1..=899).contains(&area)
        && area != 666
        && (1..=99).contains(&group)
        && (1..=9999).contains(&serial)
}

// ---------------------------------------------------------------------------------------------
// Names and filler (invented)
// ---------------------------------------------------------------------------------------------

const FIRST: [&str; 24] = [
    "Avery", "Jordan", "Riley", "Morgan", "Casey", "Quinn", "Rowan", "Sage", "Harper", "Emerson",
    "Dakota", "Finley", "Marlow", "Tatum", "Reese", "Parker", "Skyler", "Blake", "Elliot",
    "Lennox", "Wren", "Hollis", "Arden", "Bellamy",
];
const LAST: [&str; 24] = [
    "Ashford",
    "Brightwater",
    "Calloway",
    "Dunmore",
    "Everhart",
    "Fairbanks",
    "Galloway",
    "Hartwell",
    "Ironwood",
    "Jessamine",
    "Kingsley",
    "Lockridge",
    "Merriweather",
    "Northcott",
    "Oakhurst",
    "Pemberton",
    "Quillfeather",
    "Ravenscroft",
    "Stillwater",
    "Thistlewood",
    "Underhill",
    "Valmont",
    "Whitlock",
    "Yarrow",
];
const VI_FAMILY: [&str; 10] = [
    "Nguyễn", "Trần", "Lê", "Phạm", "Hoàng", "Vũ", "Đặng", "Bùi", "Đỗ", "Ngô",
];
const VI_GIVEN: [&str; 12] = [
    "Minh Anh",
    "Thị Hương",
    "Văn Hùng",
    "Quốc Bảo",
    "Thanh Tùng",
    "Ngọc Lan",
    "Hữu Phước",
    "Khánh Linh",
    "Đức Thắng",
    "Thùy Dương",
    "Hải Yến",
    "Công Minh",
];
const STREETS: [&str; 12] = [
    "Willowmere Lane",
    "Larkspur Court",
    "Tamarack Drive",
    "Fennel Street",
    "Brindle Road",
    "Quarry Hill Way",
    "Saltmarsh Avenue",
    "Cobbler Row",
    "Juniper Terrace",
    "Mossbank Place",
    "Harrow Gate",
    "Pennywhistle Lane",
];
const VI_STREETS: [&str; 6] = [
    "đường Lê Lợi",
    "ngõ 45 Trần Phú",
    "phố Hàng Bạc",
    "đường Nguyễn Huệ",
    "hẻm 12 Điện Biên Phủ",
    "đường Hoàng Diệu",
];
const CITIES: [&str; 8] = [
    "Brindleton",
    "Oakmere",
    "Fallowfield",
    "Greystone",
    "Harbourne",
    "Lindenholt",
    "Marrowby",
    "Stonebridge",
];
const STATES: [&str; 8] = ["OH", "TX", "WA", "GA", "CO", "NY", "MN", "AZ"];
pub const COMPANIES: [&str; 8] = [
    "Harborview Credit Union",
    "Northcott Savings",
    "Pemberton Trust",
    "Stillwater Bank",
    "Galloway Federal",
    "Thistlewood Mutual",
    "Oakhurst Financial",
    "Merriweather Bank",
];
pub const PRODUCTS: [&str; 8] = [
    "Copper kettle",
    "Linen apron",
    "Walnut tray",
    "Brass lantern",
    "Wool throw",
    "Slate coaster set",
    "Ceramic planter",
    "Pine step stool",
];

pub fn person(rng: &mut Rng, vi: bool) -> String {
    if vi {
        format!("{} {}", rng.pick(&VI_FAMILY), rng.pick(&VI_GIVEN))
    } else {
        format!("{} {}", rng.pick(&FIRST), rng.pick(&LAST))
    }
}

/// ASCII-only first and last name for e-mail local parts.
fn ascii_name(rng: &mut Rng) -> (String, String) {
    (
        rng.pick(&FIRST).to_lowercase(),
        rng.pick(&LAST).to_lowercase(),
    )
}

pub fn street(rng: &mut Rng, vi: bool) -> String {
    if vi {
        format!("{} {}", rng.range(1, 240), rng.pick(&VI_STREETS))
    } else {
        format!("{} {}", rng.range(10, 9899), rng.pick(&STREETS))
    }
}

pub fn city_line(rng: &mut Rng, vi: bool) -> String {
    if vi {
        let c = ["Hà Nội", "Đà Nẵng", "Cần Thơ", "Huế", "Hải Phòng"];
        format!("{}, Việt Nam", rng.pick(&c))
    } else {
        format!(
            "{}, {} {}",
            rng.pick(&CITIES),
            rng.pick(&STATES),
            rng.digits(5)
        )
    }
}

// ---------------------------------------------------------------------------------------------
// Sensitive values
// ---------------------------------------------------------------------------------------------

/// A payment card: the displayed form.
pub struct Card {
    #[cfg(test)]
    pub digits: String,
    pub shown: String,
    pub amex: bool,
}

const TEST_CARDS: [&str; 8] = [
    "4111111111111111",
    "4012888888881881",
    "5555555555554444",
    "5105105105105100",
    "378282246310005",
    "371449635398431",
    "6011111111111117",
    "6011000990139424",
];

fn with_luhn(prefix: &str, total: usize, rng: &mut Rng) -> String {
    let mut s = prefix.to_string();
    s.push_str(&rng.digits(total - 1 - prefix.len()));
    let cd = luhn_check_digit(&s);
    s.push(char::from(b'0' + cd as u8));
    s
}

fn group_card(digits: &str, amex: bool, rng: &mut Rng) -> String {
    let sep = match rng.below(20) {
        0..=11 => " ",
        12..=16 => "-",
        _ => "",
    };
    if sep.is_empty() {
        return digits.to_string();
    }
    if amex {
        format!(
            "{}{sep}{}{sep}{}",
            &digits[..4],
            &digits[4..10],
            &digits[10..]
        )
    } else {
        digits
            .as_bytes()
            .chunks(4)
            .map(|c| String::from_utf8_lossy(c).to_string())
            .collect::<Vec<_>>()
            .join(sep)
    }
}

pub fn card(rng: &mut Rng) -> Card {
    let digits = if rng.chance(0.08) {
        (*rng.pick(&TEST_CARDS)).to_string()
    } else {
        match rng.below(4) {
            0 => with_luhn("4", 16, rng),
            1 => {
                if rng.chance(0.7) {
                    with_luhn(&format!("5{}", rng.range(1, 5)), 16, rng)
                } else {
                    with_luhn(&format!("{}", rng.range(2221, 2720)), 16, rng)
                }
            }
            2 => with_luhn(if rng.chance(0.5) { "34" } else { "37" }, 15, rng),
            _ => {
                if rng.chance(0.6) {
                    with_luhn("6011", 16, rng)
                } else if rng.chance(0.5) {
                    with_luhn("65", 16, rng)
                } else {
                    with_luhn(&format!("64{}", rng.range(4, 9)), 16, rng)
                }
            }
        }
    };
    let amex = digits.len() == 15;
    let shown = group_card(&digits, amex, rng);
    Card {
        #[cfg(test)]
        digits,
        shown,
        amex,
    }
}

/// `MM/YY` expiry, a valid month and a plausible future year.
pub fn expiry(rng: &mut Rng) -> String {
    format!("{:02}/{:02}", rng.range(1, 12), rng.range(26, 33))
}

pub fn card_code(rng: &mut Rng, amex: bool) -> String {
    rng.digits(if amex { 4 } else { 3 })
}

pub fn ssn(rng: &mut Rng) -> String {
    let (a, g, s) = loop {
        let a = rng.range(1, 899) as u32;
        let g = rng.range(1, 99) as u32;
        let s = rng.range(1, 9999) as u32;
        if ssn_issuable(a, g, s) {
            break (a, g, s);
        }
    };
    let sep = if rng.chance(0.75) { "-" } else { " " };
    format!("{a:03}{sep}{g:02}{sep}{s:04}")
}

pub fn itin(rng: &mut Rng) -> String {
    let g = loop {
        let g = rng.range(70, 99);
        if g != 89 && g != 93 {
            break g;
        }
    };
    let sep = if rng.chance(0.75) { "-" } else { " " };
    format!(
        "9{:02}{sep}{g:02}{sep}{:04}",
        rng.range(0, 99),
        rng.range(0, 9999)
    )
}

pub fn ein(rng: &mut Rng) -> String {
    let p = loop {
        let p = rng.range(1, 99) as u32;
        if ein_prefix_valid(p) {
            break p;
        }
    };
    format!("{p:02}-{}", rng.digits(7))
}

pub fn aba(rng: &mut Rng) -> String {
    let p = match rng.below(4) {
        0 => rng.range(1, 12),
        1 => rng.range(21, 32),
        2 => rng.range(61, 72),
        _ => 80,
    };
    let mut s = format!("{p:02}{}", rng.digits(6));
    let d = dvals(&s);
    let sum = 3 * (d[0] + d[3] + d[6]) + 7 * (d[1] + d[4] + d[7]) + (d[2] + d[5]);
    // d[8] must make the total a multiple of ten.
    let last = (10 - sum % 10) % 10;
    s.push(char::from(b'0' + last as u8));
    s
}

fn iban_from(cc: &str, bban: &str) -> String {
    let rem = mod97(&format!("{bban}{cc}00"));
    format!("{cc}{:02}{bban}", 98 - rem)
}

pub fn iban(rng: &mut Rng) -> String {
    let raw = match rng.below(6) {
        0 => iban_from("GB", &format!("{}{}", rng.chars(4, UPPER), rng.digits(14))),
        1 => iban_from("DE", &rng.digits(18)),
        2 => iban_from("FR", &rng.digits(23)),
        3 => iban_from("ES", &rng.digits(20)),
        4 => iban_from("NL", &format!("{}{}", rng.chars(4, UPPER), rng.digits(10))),
        _ => iban_from("IT", &format!("{}{}", rng.chars(1, UPPER), rng.digits(22))),
    };
    if rng.chance(0.7) {
        raw.as_bytes()
            .chunks(4)
            .map(|c| String::from_utf8_lossy(c).to_string())
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        raw
    }
}

pub fn npi(rng: &mut Rng) -> String {
    let body = format!("{}{}", rng.range(1, 2), rng.digits(8));
    let cd = luhn_check_digit(&format!("80840{body}"));
    format!("{body}{cd}")
}

pub fn dea(rng: &mut Rng) -> String {
    let d: Vec<u32> = (0..6).map(|_| u32::from(rng.digit())).collect();
    let c = (d[0] + d[2] + d[4] + 2 * (d[1] + d[3] + d[5])) % 10;
    let digits: String = d.iter().map(|v| char::from(b'0' + *v as u8)).collect();
    format!(
        "{}{}{digits}{c}",
        rng.chars(1, "ABFGMPRX"),
        rng.chars(1, UPPER)
    )
}

pub fn mbi(rng: &mut Rng) -> String {
    let nums = "0123456789";
    let an = format!("{MBI_ALPHA}{nums}");
    let c = [
        rng.chars(1, "123456789"),
        rng.chars(1, MBI_ALPHA),
        rng.chars(1, &an),
        rng.chars(1, nums),
        rng.chars(1, MBI_ALPHA),
        rng.chars(1, &an),
        rng.chars(1, nums),
        rng.chars(1, MBI_ALPHA),
        rng.chars(1, MBI_ALPHA),
        rng.chars(1, nums),
        rng.chars(1, nums),
    ]
    .concat();
    if rng.chance(0.4) {
        format!("{}-{}-{}", &c[..4], &c[4..7], &c[7..])
    } else {
        c
    }
}

pub fn passport(rng: &mut Rng) -> String {
    format!(
        "{}{}",
        rng.chars(1, "ABCDEFGHJKLMNPRSTUVWXYZ"),
        rng.digits(8)
    )
}

pub fn vin(rng: &mut Rng) -> String {
    let al = VIN_ALPHA.to_string();
    let an = format!("{al}0123456789");
    let mut v: Vec<char> = format!(
        "{}{}{}{}0{}{}{}",
        rng.chars(1, "12345JKLSWYZ"),
        rng.chars(1, &an),
        rng.chars(1, &an),
        rng.chars(5, &an),
        rng.chars(1, "ABCDEFGHJKLMNPRSTVWXY123456789"),
        rng.chars(1, &an),
        rng.digits(6)
    )
    .chars()
    .collect();
    v[8] = vin_check_char(&v);
    v.into_iter().collect()
}

pub fn imei(rng: &mut Rng) -> String {
    let tac = format!("{}{}", rng.pick(&["35", "86", "49", "01"]), rng.digits(6));
    let body = format!("{tac}{}", rng.digits(6));
    let cd = luhn_check_digit(&body);
    format!("{body}{cd}")
}

/// Fictional phone numbers: the 555-01xx block set aside for fiction, or the 800 forms.
pub fn phone(rng: &mut Rng, vi: bool) -> String {
    if vi {
        let pre = rng.pick(&[
            "032", "035", "038", "056", "070", "079", "081", "090", "093", "098",
        ]);
        let rest = rng.digits(7);
        return match rng.below(6) {
            0 => format!("{pre}{rest}"),
            1 => format!("{} {} {}", pre, &rest[..3], &rest[3..]),
            2 => format!("{}.{}.{}", pre, &rest[..3], &rest[3..]),
            3 => format!("+84 {} {} {}", &pre[1..], &rest[..3], &rest[3..]),
            4 => format!("(+84) {}{rest}", &pre[1..]),
            _ => format!("{}{} {} {}", &pre[..3], &rest[..1], &rest[1..4], &rest[4..]),
        };
    }
    let area = if rng.chance(0.2) {
        "800".to_string()
    } else {
        loop {
            let a = format!("{}{}{}", rng.range(2, 9), rng.range(0, 8), rng.digit());
            if &a[1..] != "11" {
                break a;
            }
        }
    };
    let line = format!("01{:02}", rng.range(0, 99));
    match rng.below(6) {
        0 => format!("({area}) 555-{line}"),
        1 => format!("{area}-555-{line}"),
        2 => format!("{area}.555.{line}"),
        3 => format!("+1 {area} 555 {line}"),
        4 => format!("1-{area}-555-{line}"),
        _ => format!("+1 ({area}) 555-{line}"),
    }
}

pub fn email(rng: &mut Rng) -> String {
    let (f, l) = ascii_name(rng);
    let local = match rng.below(4) {
        0 => format!("{f}.{l}"),
        1 => format!("{f}_{l}"),
        2 => format!("{}{l}{}", &f[..1], rng.below(100)),
        _ => format!("{l}{}", rng.below(1000)),
    };
    let dom = rng.pick(&[
        "example.com",
        "example.org",
        "example.net",
        "mail.test",
        "corp.invalid",
        "intranet.test",
        "sub.example.com",
    ]);
    format!("{local}@{dom}")
}

pub fn ipv4(rng: &mut Rng) -> String {
    let net = rng.pick(&["192.0.2", "198.51.100", "203.0.113"]);
    format!("{net}.{}", rng.range(1, 254))
}

pub fn ipv6(rng: &mut Rng) -> String {
    let mut g = [0x2001u32, 0x0db8, 0, 0, 0, 0, 0, 0];
    for slot in g.iter_mut().skip(2) {
        if rng.chance(0.55) {
            *slot = rng.range(1, 0xffff) as u32;
        }
    }
    if g[7] == 0 {
        g[7] = rng.range(1, 0xffff) as u32;
    }
    let hex = |s: &[u32]| {
        s.iter()
            .map(|v| format!("{v:x}"))
            .collect::<Vec<_>>()
            .join(":")
    };
    if rng.chance(0.25) {
        return hex(&g);
    }
    // compress the longest run of zero groups
    let (mut best, mut best_len, mut i) = (0usize, 0usize, 0usize);
    while i < 8 {
        if g[i] == 0 {
            let s = i;
            while i < 8 && g[i] == 0 {
                i += 1;
            }
            if i - s > best_len {
                best = s;
                best_len = i - s;
            }
        } else {
            i += 1;
        }
    }
    if best_len >= 2 {
        format!("{}::{}", hex(&g[..best]), hex(&g[best + best_len..]))
    } else {
        hex(&g)
    }
}

pub fn mac(rng: &mut Rng) -> String {
    let b: Vec<u32> = if rng.chance(0.5) {
        vec![0x00, 0x00, 0x5e, 0x00, 0x53, rng.range(0, 255) as u32]
    } else {
        let mut v = vec![0x02];
        v.extend((0..5).map(|_| rng.range(0, 255) as u32));
        v
    };
    match rng.below(4) {
        0 | 1 => b
            .iter()
            .map(|v| format!("{v:02x}"))
            .collect::<Vec<_>>()
            .join(":"),
        2 => b
            .iter()
            .map(|v| format!("{v:02X}"))
            .collect::<Vec<_>>()
            .join("-"),
        _ => b
            .chunks(2)
            .map(|c| format!("{:02x}{:02x}", c[0], c[1]))
            .collect::<Vec<_>>()
            .join("."),
    }
}

/// A provider-shaped key from random characters (no real key).
pub fn token(rng: &mut Rng) -> String {
    match rng.below(9) {
        0 => format!("AKIA{}", rng.chars(16, UPNUM)),
        1 => format!("ghp_{}", rng.chars(36, ALNUM)),
        2 => format!(
            "xoxb-{}-{}-{}",
            rng.digits(12),
            rng.digits(12),
            rng.chars(24, ALNUM)
        ),
        3 => format!("sk_live_{}", rng.chars(24, ALNUM)),
        4 => format!("AIza{}", rng.chars(35, B64URL)),
        5 => format!("sk-{}", rng.chars(48, ALNUM)),
        6 => format!("glpat-{}", rng.chars(20, B64URL)),
        7 => format!("sk_test_{}", rng.chars(24, ALNUM)),
        _ => format!(
            "eyJ{}.eyJ{}.{}",
            rng.chars(33, B64URL),
            rng.chars(38, B64URL),
            rng.chars(30, B64URL)
        ),
    }
}

const SECRET_NAMES: [&str; 12] = [
    "DB_PASSWORD",
    "API_SECRET",
    "SECRET_KEY",
    "AUTH_TOKEN",
    "PGPASSWORD",
    "STRIPE_SECRET",
    "JWT_SECRET",
    "REDIS_PASSWORD",
    "CLIENT_SECRET",
    "ADMIN_PASSWORD",
    "SMTP_PASSWORD",
    "SESSION_SECRET",
];
const JSON_SECRET_NAMES: [&str; 5] = [
    "password",
    "api_key",
    "secret",
    "client_secret",
    "access_token",
];

/// `(name, value)` of a secret assignment; `json` picks lower-case JSON names.
pub fn secret_pair(rng: &mut Rng, json: bool) -> (String, String) {
    let name = if json {
        (*rng.pick(&JSON_SECRET_NAMES)).to_string()
    } else {
        (*rng.pick(&SECRET_NAMES)).to_string()
    };
    let n = rng.range(14, 30) as usize;
    let alpha = if rng.chance(0.5) { ALNUM } else { LOWNUM };
    (name, rng.chars(n, alpha))
}

pub fn url_secret(rng: &mut Rng) -> String {
    let sub = rng.pick(&["api", "db", "files", "git", "cdn"]);
    match rng.below(4) {
        0 => format!(
            "https://{}:{}@{sub}.example.com/{}",
            rng.pick(&["deploy", "admin", "svc", "ci"]),
            rng.chars(12, ALNUM),
            rng.pick(&["repo.git", "v2/export", "backup"])
        ),
        1 => format!(
            "https://{sub}.example.com/v1/export?access_token={}",
            rng.chars(24, ALNUM)
        ),
        2 => format!(
            "https://{sub}.example.org/hook?api_key={}&format=json",
            rng.chars(20, LOWNUM)
        ),
        _ => format!(
            "postgres://{}:{}@db.example.com:5432/app",
            rng.pick(&["app", "postgres", "reporting"]),
            rng.chars(14, ALNUM)
        ),
    }
}

/// A PEM private-key block, one string per line.
pub fn pem_block(rng: &mut Rng, body_lines: usize) -> Vec<String> {
    let kind = rng.pick(&[
        "PRIVATE KEY",
        "RSA PRIVATE KEY",
        "OPENSSH PRIVATE KEY",
        "EC PRIVATE KEY",
    ]);
    let mut v = vec![format!("-----BEGIN {kind}-----")];
    for i in 0..body_lines {
        if i + 1 == body_lines {
            let n = rng.range(20, 60) as usize;
            let mut l = rng.chars(n, B64);
            if rng.chance(0.5) {
                l.push('=');
            }
            v.push(l);
        } else {
            v.push(rng.chars(64, B64));
        }
    }
    v.push(format!("-----END {kind}-----"));
    v
}

// ---------------------------------------------------------------------------------------------
// Decoys
// ---------------------------------------------------------------------------------------------

/// Decoys that look like `kind` (so the negatives are hard ones).
pub fn related_decoys(kind: &str) -> &'static [&'static str] {
    match kind {
        "card" => &["luhn_fail_16_digits", "masked_card", "tracking_number"],
        "ssn" => &["masked_ssn", "order_number", "part_number"],
        "itin" | "ein" => &["order_number", "invoice_number", "part_number"],
        "aba" => &["order_number", "part_number", "table_number"],
        "iban" => &["iban_fail", "tracking_number"],
        "npi" => &["npi_fail", "ten_digit_id", "part_number"],
        "dea" | "mbi" | "passport" => &["part_number", "build_number"],
        "vin" => &["vin_fail", "git_hash"],
        "imei" => &["imei_fail", "tracking_number"],
        "ip_address" => &["version_string", "coordinates"],
        "mac_address" => &["uuid", "short_hash"],
        "phone" => &["ten_digit_id", "timestamp", "epoch_ms"],
        "email" => &["plain_url", "version_string"],
        "token" => &["placeholder_secret"],
        "secret_assignment" => &["placeholder_secret"],
        "url_secret" => &["plain_url"],
        "private_key" => &["git_hash", "short_hash"],
        _ => &["order_number", "bare_date"],
    }
}

fn fail_luhn(rng: &mut Rng, prefix: &str, total: usize) -> String {
    loop {
        let s = format!("{prefix}{}", rng.digits(total - prefix.len()));
        if !luhn_valid(&s) {
            return s;
        }
    }
}

fn group4(s: &str, rng: &mut Rng) -> String {
    let sep = if rng.chance(0.7) { " " } else { "-" };
    s.as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).to_string())
        .collect::<Vec<_>>()
        .join(sep)
}

/// The text of a decoy of the named kind.
pub fn decoy(what: &str, rng: &mut Rng) -> String {
    match what {
        "order_number" => {
            let n = loop {
                let n = rng.digits(9);
                if !aba_valid(&n) {
                    break n;
                }
            };
            match rng.below(3) {
                0 => n,
                1 => format!("#{n}"),
                _ => format!("ORD-{n}"),
            }
        }
        "part_number" => format!(
            "{}-{}-{:02}",
            rng.chars(2, UPPER),
            rng.digits(5),
            rng.range(1, 99)
        ),
        "tracking_number" => match rng.below(3) {
            0 => format!(
                "1Z{}{}{}",
                rng.chars(3, UPNUM),
                rng.chars(3, UPNUM),
                rng.digits(10)
            ),
            1 => group4(&format!("9400{}", rng.digits(18)), rng),
            _ => rng.digits(12),
        },
        "uuid" => format!(
            "{}-{}-4{}-{}{}-{}",
            rng.chars(8, HEX),
            rng.chars(4, HEX),
            rng.chars(3, HEX),
            rng.chars(1, "89ab"),
            rng.chars(3, HEX),
            rng.chars(12, HEX)
        ),
        "git_hash" => rng.chars(40, HEX),
        "short_hash" => rng.chars(7, HEX),
        "version_string" => match rng.below(3) {
            0 => format!("v{}.{}.{}", rng.below(5), rng.below(30), rng.below(200)),
            1 => format!(
                "{}.{}.{}.{}",
                rng.range(1, 12),
                rng.below(10),
                rng.below(10),
                rng.range(1000, 9999)
            ),
            _ => format!(
                "{}.{}.{}-rc.{}",
                rng.below(4),
                rng.below(20),
                rng.below(20),
                rng.range(1, 9)
            ),
        },
        "semver_range" => format!("^{}.{}.{}", rng.below(5), rng.below(30), rng.below(40)),
        "timestamp" => match rng.below(3) {
            0 => format!(
                "2026-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                rng.range(1, 12),
                rng.range(1, 28),
                rng.range(0, 23),
                rng.range(0, 59),
                rng.range(0, 59)
            ),
            1 => format!(
                "2026-{:02}-{:02} {:02}:{:02}:{:02}",
                rng.range(1, 12),
                rng.range(1, 28),
                rng.range(0, 23),
                rng.range(0, 59),
                rng.range(0, 59)
            ),
            _ => format!("17{}", rng.digits(8)),
        },
        "epoch_ms" => format!("17{}", rng.digits(11)),
        "isbn" => {
            let body = format!("978{}", rng.digits(9));
            let sum: u32 = dvals(&body)
                .iter()
                .enumerate()
                .map(|(i, d)| if i % 2 == 0 { *d } else { 3 * d })
                .sum();
            let cd = (10 - sum % 10) % 10;
            format!(
                "{}-{}-{}-{}-{cd}",
                &body[..3],
                &body[3..4],
                &body[4..8],
                &body[8..]
            )
        }
        "luhn_fail_16_digits" => {
            let pre = *rng.pick(&["4", "51", "52", "6011", "35"]);
            let n = fail_luhn(rng, pre, 16);
            group4(&n, rng)
        }
        "coordinates" => format!(
            "{:.6}, -{:.6}",
            rng.range(25, 48) as f64 + rng.range(0, 999_999) as f64 / 1e6,
            rng.range(70, 124) as f64 + rng.range(0, 999_999) as f64 / 1e6
        ),
        "dollar_amount" => {
            let whole = rng.range(1, 99_999);
            let s = whole.to_string();
            let mut grouped = String::new();
            for (i, c) in s.chars().enumerate() {
                if i > 0 && (s.len() - i).is_multiple_of(3) {
                    grouped.push(',');
                }
                grouped.push(c);
            }
            format!("${grouped}.{:02}", rng.range(0, 99))
        }
        "table_number" => {
            let n = rng.range(6, 9) as usize;
            format!("{}7", rng.digits(n).trim_start_matches('0'))
        }
        "bare_date" => match rng.below(3) {
            0 => format!(
                "{:02}/{:02}/{}",
                rng.range(1, 12),
                rng.range(1, 28),
                rng.range(1950, 2026)
            ),
            1 => format!(
                "{}-{:02}-{:02}",
                rng.range(1950, 2026),
                rng.range(1, 12),
                rng.range(1, 28)
            ),
            _ => format!(
                "{} {}, {}",
                rng.pick(&[
                    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov",
                    "Dec"
                ]),
                rng.range(1, 28),
                rng.range(1950, 2026)
            ),
        },
        "masked_ssn" => {
            let mark = rng.pick(&["***-**-", "XXX-XX-", "•••-••-"]);
            format!("{mark}{}", rng.digits(4))
        }
        "masked_card" => match rng.below(3) {
            0 => format!("**** **** **** {}", rng.digits(4)),
            1 => format!("•••• {}", rng.digits(4)),
            _ => format!("XXXX-XXXX-XXXX-{}", rng.digits(4)),
        },
        "masked_account" => format!("••••{}", rng.digits(4)),
        "placeholder_secret" => (*rng.pick(&[
            "your_api_key_here",
            "changeme",
            "<YOUR_TOKEN>",
            "xxxxxxxxxxxxxxxx",
            "REPLACE_ME",
            "{{SECRET}}",
            "********",
            "example-secret",
        ]))
        .to_string(),
        "vin_fail" => {
            let mut v: Vec<char> = vin(rng).chars().collect();
            let right = v[8];
            v[8] = loop {
                let c = rng.chars(1, "0123456789X").chars().next().unwrap_or('0');
                if c != right {
                    break c;
                }
            };
            v.into_iter().collect()
        }
        "imei_fail" => fail_luhn(rng, "35", 15),
        "npi_fail" => loop {
            let s = format!("{}{}", rng.range(1, 2), rng.digits(9));
            if !npi_valid(&s) {
                break s;
            }
        },
        "iban_fail" => {
            let mut s: Vec<char> = iban(rng).chars().filter(|c| *c != ' ').collect();
            let d = s[2].to_digit(10).unwrap_or(0);
            s[2] = char::from(b'0' + ((d + 3) % 10) as u8);
            let s: String = s.into_iter().collect();
            s
        }
        "plain_url" => format!(
            "https://{}.example.com/{}",
            rng.pick(&["docs", "www", "status"]),
            rng.pick(&[
                "guide/getting-started",
                "pricing",
                "blog/release-notes",
                "help?page=2"
            ])
        ),
        "invoice_number" => format!("INV-2026-{}", rng.digits(6)),
        "ten_digit_id" => loop {
            let s = format!("{}{}", rng.range(1, 9), rng.digits(9));
            if !npi_valid(&s) {
                break s;
            }
        },
        "lone_expiry" => expiry(rng),
        "lone_code" => rng.digits(3),
        "build_number" => format!("build {}", rng.range(1000, 99999)),
        "hex_color" => format!("#{}", rng.chars(6, "0123456789ABCDEF")),
        "file_size" => format!("{:.1} MB", rng.range(1, 999) as f64 / 10.0),
        "percentage" => format!("{:.1}%", rng.range(1, 999) as f64 / 10.0),
        _ => rng.digits(9),
    }
}

/// Standards that name each kind (same table as the detector's README mapping; written here
/// again on purpose, see the module comment).
pub fn standards(kind: &str) -> Vec<String> {
    let v: &[&str] = match kind {
        "email" => &["HIPAA 6"],
        "token" | "private_key" | "secret_assignment" => {
            &["GLBA / state breach laws (credentials)"]
        }
        "url_secret" => &["HIPAA 14", "GLBA / state breach laws (credentials)"],
        "card" => &["PCI DSS", "HIPAA 10", "GLBA"],
        "card_extra" => &["PCI DSS (sensitive authentication data)"],
        "ssn" | "itin" => &["NIST SP 800-122", "HIPAA 7", "GLBA"],
        "ein" => &["NIST SP 800-122"],
        "aba" | "iban" => &["HIPAA 10", "GLBA"],
        "npi" | "dea" => &["HIPAA 11"],
        "mbi" => &["HIPAA 9"],
        "passport" => &["HIPAA 11", "GLBA"],
        "vin" => &["HIPAA 12"],
        "imei" | "mac_address" => &["HIPAA 13"],
        "ip_address" => &["HIPAA 15"],
        "phone" => &["HIPAA 4, 5"],
        _ => &[],
    };
    v.iter().map(|s| (*s).to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors() {
        assert!(luhn_valid("4111111111111111"));
        assert!(!luhn_valid("4111111111111112"));
        assert!(aba_valid("021000021"));
        assert!(!aba_valid("021000022"));
        assert!(iban_valid("GB82WEST12345698765432"));
        assert!(iban_valid("DE89 3704 0044 0532 0130 00"));
        assert!(npi_valid("1234567893"));
        assert!(dea_valid("AB1234563"));
        assert!(vin_valid("1M8GDM9AXKP042788"));
        assert!(luhn_valid("490154203237518"));
        assert!(mbi_valid("1EG4-TE5-MK73"));
        assert!(!mbi_valid("1SG4-TE5-MK73"));
    }

    #[test]
    fn generators_satisfy_their_checksums() {
        let mut r = Rng::new(7);
        for _ in 0..300 {
            let c = card(&mut r);
            assert!(luhn_valid(&c.digits), "{}", c.digits);
            assert!(matches!(c.digits.len(), 15 | 16));
            let first = c.digits.as_bytes()[0];
            assert!(matches!(first, b'3' | b'4' | b'5' | b'6' | b'2'));
            assert!(aba_valid(&aba(&mut r)));
            assert!(iban_valid(&iban(&mut r)));
            assert!(npi_valid(&npi(&mut r)));
            assert!(dea_valid(&dea(&mut r)));
            assert!(mbi_valid(&mbi(&mut r)));
            assert!(vin_valid(&vin(&mut r)));
            let im = imei(&mut r);
            assert!(im.len() == 15 && luhn_valid(&im));
        }
    }

    #[test]
    fn identifiers_stay_in_their_ranges() {
        let mut r = Rng::new(11);
        for _ in 0..300 {
            let s = ssn(&mut r);
            let d: Vec<u32> = s.split(['-', ' ']).map(|p| p.parse().unwrap()).collect();
            assert!(ssn_issuable(d[0], d[1], d[2]), "{s}");
            let t = itin(&mut r);
            assert!(t.starts_with('9'), "{t}");
            let g: u32 = t.split(['-', ' ']).nth(1).unwrap().parse().unwrap();
            assert!((70..=99).contains(&g) && g != 89 && g != 93, "{t}");
            let e = ein(&mut r);
            assert!(ein_prefix_valid(e[..2].parse().unwrap()), "{e}");
            let p = phone(&mut r, false);
            assert!(p.contains("555") && p.contains("01"), "{p}");
            let ip = ipv4(&mut r);
            assert!(
                ip.starts_with("192.0.2.")
                    || ip.starts_with("198.51.100.")
                    || ip.starts_with("203.0.113."),
                "{ip}"
            );
            assert!(ipv6(&mut r).starts_with("2001:db8:"));
            let em = email(&mut r);
            assert!(
                [
                    "example.com",
                    "example.org",
                    "example.net",
                    ".test",
                    ".invalid"
                ]
                .iter()
                .any(|d| em.ends_with(d)),
                "{em}"
            );
        }
    }

    #[test]
    fn decoys_do_not_pass_the_rules_they_imitate() {
        let mut r = Rng::new(3);
        for _ in 0..200 {
            let c = decoy("luhn_fail_16_digits", &mut r);
            assert!(!luhn_valid(&c), "{c}");
            assert!(!imei_ok(&decoy("imei_fail", &mut r)));
            assert!(!vin_valid(&decoy("vin_fail", &mut r)));
            assert!(!npi_valid(&decoy("npi_fail", &mut r)));
            assert!(!iban_valid(&decoy("iban_fail", &mut r)));
        }
    }

    fn imei_ok(s: &str) -> bool {
        luhn_valid(s)
    }
}
