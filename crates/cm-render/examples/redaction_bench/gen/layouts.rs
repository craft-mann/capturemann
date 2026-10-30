//! The screen layouts. Each draws one fake application screen in logical px with `Ctx`; every
//! sensitive value goes through `Ctx::value` and friends, so a negative picture can never
//! receive a truth. A layout draws rows until its canvas is full, so smaller fonts give more rows.

use cm_core::Color;

use super::data;
use super::scene::{d, p, Ctx, Seg};

pub struct Spec {
    pub name: &'static str,
    /// Whole layouts are held out, never single pictures.
    pub held_out: bool,
    pub size: (u32, u32),
    /// The layout has Vietnamese labels (`Ctx::tr`).
    pub vi: bool,
    pub draw: fn(&mut Ctx),
}

pub const LAYOUTS: [Spec; 13] = [
    Spec {
        name: "bank_statement",
        held_out: false,
        size: (900, 640),
        vi: true,
        draw: bank_statement,
    },
    Spec {
        name: "insurance_card",
        held_out: true,
        size: (860, 560),
        vi: false,
        draw: insurance_card,
    },
    Spec {
        name: "patient_intake",
        held_out: false,
        size: (820, 700),
        vi: true,
        draw: patient_intake,
    },
    Spec {
        name: "mail_client",
        held_out: false,
        size: (1000, 640),
        vi: true,
        draw: mail_client,
    },
    Spec {
        name: "chat",
        held_out: true,
        size: (760, 640),
        vi: true,
        draw: chat,
    },
    Spec {
        name: "crm_table",
        held_out: false,
        size: (1100, 600),
        vi: true,
        draw: crm_table,
    },
    Spec {
        name: "hr_sheet",
        held_out: true,
        size: (1100, 620),
        vi: false,
        draw: hr_sheet,
    },
    Spec {
        name: "terminal",
        held_out: false,
        size: (880, 560),
        vi: false,
        draw: terminal,
    },
    Spec {
        name: "env_file",
        held_out: false,
        size: (860, 600),
        vi: false,
        draw: env_file,
    },
    Spec {
        name: "devtools",
        held_out: true,
        size: (1000, 620),
        vi: false,
        draw: devtools,
    },
    Spec {
        name: "log_viewer",
        held_out: false,
        size: (1000, 600),
        vi: false,
        draw: log_viewer,
    },
    Spec {
        name: "settings_page",
        held_out: false,
        size: (860, 640),
        vi: true,
        draw: settings_page,
    },
    Spec {
        name: "checkout",
        held_out: false,
        size: (760, 640),
        vi: false,
        draw: checkout,
    },
];

// ----- small helpers -----

fn put(c: &mut Ctx, x: f32, y: f32, px: f32, col: Color, segs: &[Seg]) -> f32 {
    c.line(x, y, px, col, segs).map_or(0.0, |pl| pl.w)
}

fn txt(c: &mut Ctx, x: f32, y: f32, px: f32, col: Color, s: &str) -> f32 {
    put(c, x, y, px, col, &[p(s)])
}

/// `label: value` on one line.
fn lv(c: &mut Ctx, x: f32, y: f32, px: f32, label: &str, mut val: Vec<Seg>) -> f32 {
    let col = c.th.fg;
    let mut segs = vec![p(format!("{label}: "))];
    segs.append(&mut val);
    put(c, x, y, px, col, &segs)
}

fn dec(what: &'static str, c: &mut Ctx) -> Seg {
    d(what, data::decoy(what, &mut c.rng))
}

fn pick_decoy(c: &mut Ctx, kinds: &[&'static str]) -> Seg {
    let w = *c.rng.pick(kinds);
    dec(w, c)
}

fn titlebar(c: &mut Ctx, title: &str) -> f32 {
    let (bar, border, muted, w) = (c.th.panel2, c.th.border, c.th.muted, c.w);
    c.fill(0.0, 0.0, w, 28.0, bar);
    c.hline(0.0, 28.0, w, border);
    for (i, col) in [
        Color::rgb(238, 95, 86),
        Color::rgb(245, 191, 79),
        Color::rgb(98, 197, 84),
    ]
    .into_iter()
    .enumerate()
    {
        c.fill(10.0 + i as f32 * 16.0, 9.0, 10.0, 10.0, col);
    }
    let tw = c.measure(11.0, title);
    txt(c, (w - tw) / 2.0, 7.0, 11.0, muted, title);
    28.0
}

/// A one-line message built from templates; values go through the usual gates.
fn snippet(c: &mut Ctx) -> Vec<Seg> {
    let vi = c.vi;
    match c.rng.below(13) {
        0 => {
            let mut v = vec![p(c.tr("my card is ", "thẻ của tôi là "))];
            v.extend(c.card_inline());
            v
        }
        1 => {
            let mut v = vec![p("SSN ")];
            v.extend(c.value(&["ssn", "itin"]));
            v
        }
        2 => {
            let mut v = vec![p(c.tr("call me on ", "gọi cho tôi theo số "))];
            v.extend(c.value(&["phone"]));
            v
        }
        3 => {
            let mut v = vec![p(c.tr("the box is at ", "máy chủ ở "))];
            v.extend(c.value(&["ip_address"]));
            v
        }
        4 => {
            let mut v = vec![p("token: ")];
            v.extend(c.value_w(&["token"], 60));
            v
        }
        5 => {
            let mut v = vec![p(c.tr("email me at ", "gửi email cho tôi tại "))];
            v.extend(c.value(&["email"]));
            v
        }
        6 => vec![
            p(c.tr("order ", "đơn hàng ")),
            dec("order_number", c),
            p(c.tr(" shipped, tracking ", " đã gửi, mã vận đơn ")),
            dec("tracking_number", c),
        ],
        7 => {
            let words = if vi {
                [
                    "Cảm ơn bạn, mình sẽ kiểm tra rồi báo lại.",
                    "Họp lúc 3 giờ chiều nhé.",
                    "Đã gửi tệp đính kèm.",
                ]
            } else {
                [
                    "Thanks, I will check and get back to you.",
                    "Meeting moved to 3pm.",
                    "Attached the file.",
                ]
            };
            vec![p(*c.rng.pick(&words))]
        }
        8 => {
            let mut v = vec![p(c.tr("routing number ", "số định tuyến "))];
            v.extend(c.value(&["aba"]));
            v
        }
        9 => {
            let mut v = vec![p("passport ")];
            v.extend(c.value(&["passport"]));
            v
        }
        10 => vec![
            p(c.tr("total due ", "tổng cộng ")),
            dec("dollar_amount", c),
            p(c.tr(" by ", " trước ")),
            dec("bare_date", c),
        ],
        11 => {
            let mut v = vec![p("")];
            v.extend(c.secret_line(false));
            v
        }
        _ => {
            let mut v = vec![p("IBAN ")];
            v.extend(c.value(&["iban"]));
            v
        }
    }
}

fn join(segs: &[Seg]) -> String {
    segs.iter().map(|s| s.text.as_str()).collect()
}

// ----- 1. bank statement -----

fn bank_statement(c: &mut Ctx) {
    let (px, th) = (c.px, c.th);
    let w = c.w;
    c.fill(0.0, 0.0, w, 56.0, th.accent);
    let bank = *c.rng.pick(&data::COMPANIES);
    txt(c, 24.0, 15.0, px + 8.0, Color::WHITE, bank);
    let period = [
        p(format!("{}: ", c.tr("Statement period", "Kỳ sao kê"))),
        dec("bare_date", c),
        p(" - "),
        dec("bare_date", c),
    ];
    let pw = c.measure(px, &join(&period));
    put(c, w - pw - 24.0, 20.0, px, Color::WHITE, &period);

    c.panel(20.0, 72.0, w - 40.0, 40.0 + px * 9.6);
    let lh = px * 1.6;
    let mut y = 84.0;
    let holder = c.person();
    txt(c, 36.0, y, px + 2.0, th.fg, &holder);
    y += lh + 2.0;
    let street = data::street(&mut c.rng, c.vi);
    let city = data::city_line(&mut c.rng, c.vi);
    txt(c, 36.0, y, px, th.muted, &street);
    txt(c, 36.0, y + lh, px, th.muted, &city);
    y += lh * 2.0;
    let rows: [(&str, &str, &[&'static str]); 3] = [
        ("Routing number", "Số định tuyến", &["aba"]),
        ("Phone", "Điện thoại", &["phone"]),
        ("Email", "Email", &["email"]),
    ];
    for (en, vi, kinds) in rows {
        let v = c.value(kinds);
        let label = c.tr(en, vi);
        lv(c, 36.0, y, px, label, v);
        y += lh;
    }
    let acct = vec![dec("masked_account", c)];
    let label = c.tr("Account", "Tài khoản");
    lv(c, 36.0, y, px, label, acct);
    // right half: debit card and an IBAN
    let mut y2 = 84.0 + lh * 2.0;
    let mut v = vec![p(format!("{} ", c.tr("Debit card", "Thẻ ghi nợ")))];
    v.extend(c.card_inline());
    put(c, w / 2.0 - 20.0, y2, px, th.fg, &v);
    y2 += lh;
    let v = c.value_w(&["iban"], 34);
    lv(c, w / 2.0 - 20.0, y2, px, "IBAN", v);

    // transaction table
    let mut y = 72.0 + 40.0 + px * 9.6 + 16.0;
    let xs = [24.0, 120.0, w * 0.44, w * 0.72, w - 110.0];
    for (i, h) in ["Date", "Description", "Reference", "Amount", "Balance"]
        .into_iter()
        .enumerate()
    {
        txt(c, xs[i], y, px, th.muted, h);
    }
    y += lh;
    c.hline(20.0, y - 3.0, w - 40.0, th.border);
    let mut n = 0;
    while y + lh < c.h - 8.0 {
        if n % 2 == 1 {
            c.fill_a(20.0, y - 2.0, w - 40.0, lh, th.panel2, 120);
        }
        let date = dec("bare_date", c);
        put(c, xs[0], y, px, th.fg, &[date]);
        let desc = format!("{} #{}", c.rng.pick(&data::PRODUCTS), c.rng.below(90) + 10);
        txt(c, xs[1], y, px, th.fg, &desc);
        let r = pick_decoy(
            c,
            &[
                "order_number",
                "tracking_number",
                "part_number",
                "invoice_number",
            ],
        );
        put(c, xs[2], y, px, th.fg, &[r]);
        let a = dec("dollar_amount", c);
        put(c, xs[3], y, px, th.fg, &[a]);
        let b = dec("dollar_amount", c);
        put(c, xs[4], y, px, th.fg, &[b]);
        y += lh;
        n += 1;
    }
}

// ----- 2. insurance card page -----

fn insurance_card(c: &mut Ctx) {
    let (px, th, w) = (c.px, c.th, c.w);
    txt(c, 28.0, 18.0, px + 8.0, th.fg, "My Coverage");
    let cw = (w - 84.0) / 2.0;
    let ch = 30.0 + px * 19.5;
    for (i, x) in [28.0, 56.0 + cw].into_iter().enumerate() {
        c.panel(x, 56.0, cw, ch);
        c.fill(x, 56.0, cw, 6.0, th.accent);
        let lh = px * 2.6;
        let mut y = 74.0;
        let field = |c: &mut Ctx, y: &mut f32, label: &str, val: Vec<Seg>| {
            txt(c, x + 14.0, *y, px, th.muted, label);
            put(c, x + 14.0, *y + px * 1.25, px + 1.0, th.fg, &val);
            *y += lh;
        };
        if i == 0 {
            let plan = format!("{} PPO", c.rng.pick(&data::COMPANIES));
            txt(c, x + 14.0, y, px + 4.0, th.accent, &plan);
            y += lh;
            let n = c.person();
            field(c, &mut y, "Member", vec![p(n)]);
            let v = c.value(&["mbi"]);
            field(c, &mut y, "Medicare / Member ID", v);
            let grp = format!("GRP-{}", c.rng.digits(5));
            field(c, &mut y, "Group", vec![p(grp)]);
            let v = c.value(&["npi"]);
            field(c, &mut y, "Primary care provider NPI", v);
            let v = c.value(&["dea"]);
            field(c, &mut y, "Prescriber DEA", v);
            let v = c.value(&["ssn"]);
            field(c, &mut y, "Subscriber SSN", v);
        } else {
            let v = c.value(&["phone"]);
            field(c, &mut y, "Member services", v);
            let v = c.value(&["phone"]);
            field(c, &mut y, "Provider line", v);
            let v = c.value(&["email"]);
            field(c, &mut y, "Claims email", v);
            let street = data::street(&mut c.rng, false);
            field(c, &mut y, "Claims address", vec![p(street)]);
            let v = c.value(&["mbi", "ein"]);
            field(c, &mut y, "Plan tax ID", v);
            let (a, b) = c.card_two_lines();
            field(c, &mut y, "Copay card", a);
            put(c, x + 14.0, y + px * 0.15, px, th.fg, &b);
        }
    }
    let y = 56.0 + ch + 18.0;
    txt(c, 28.0, y, px + 2.0, th.fg, "Recent claims");
    let mut yy = y + px * 2.2;
    while yy + px * 1.6 < c.h - 8.0 {
        let a = dec("bare_date", c);
        let b = pick_decoy(c, &["order_number", "invoice_number", "part_number"]);
        let e = dec("dollar_amount", c);
        put(c, 28.0, yy, px, th.fg, &[a]);
        put(c, 140.0, yy, px, th.fg, &[b]);
        put(c, 330.0, yy, px, th.fg, &[e]);
        yy += px * 1.6;
    }
}

// ----- 3. patient intake form -----

fn patient_intake(c: &mut Ctx) {
    let (px, th, w) = (c.px, c.th, c.w);
    let title = c.tr("Patient intake form", "Phiếu tiếp nhận bệnh nhân");
    txt(c, 32.0, 20.0, px + 9.0, th.fg, title);
    let colw = (w - 96.0) / 2.0;
    let fields: [(&str, &str, &[&'static str]); 12] = [
        ("Full name", "Họ và tên", &[]),
        ("Date of birth", "Ngày sinh", &["@date"]),
        ("Social Security number", "Số an sinh", &["ssn", "itin"]),
        ("Phone", "Điện thoại", &["phone"]),
        ("Email", "Email", &["email"]),
        ("Address", "Địa chỉ", &["@addr"]),
        ("Insurance ID", "Mã bảo hiểm", &["mbi"]),
        ("Passport number", "Số hộ chiếu", &["passport"]),
        ("Emergency contact", "Liên hệ khẩn cấp", &["phone"]),
        ("Referring physician NPI", "NPI bác sĩ giới thiệu", &["npi"]),
        ("Employer EIN", "Mã số thuế", &["ein"]),
        ("Preferred pharmacy", "Nhà thuốc", &["@plain"]),
    ];
    let rowh = px * 4.6;
    for (i, (en, vi, kinds)) in fields.into_iter().enumerate() {
        let col = i % 2;
        let row = i / 2;
        let x = 32.0 + col as f32 * (colw + 32.0);
        let y = 66.0 + row as f32 * rowh;
        if y + rowh > c.h {
            break;
        }
        let label = c.tr(en, vi);
        txt(c, x, y, px, th.muted, label);
        let (bg, bd) = (th.panel, th.border);
        c.fill(x, y + px * 1.5, colw, px * 2.4, bg);
        c.frame(x, y + px * 1.5, colw, px * 2.4, bd);
        let val = match kinds.first() {
            Some(&"@date") => vec![dec("bare_date", c)],
            Some(&"@addr") => vec![p(format!(
                "{}, {}",
                data::street(&mut c.rng, c.vi),
                data::city_line(&mut c.rng, c.vi)
            ))],
            Some(&"@plain") => vec![p(c.rng.pick(&data::COMPANIES).to_string())],
            Some(_) => c.value(kinds),
            None => vec![p(c.person())],
        };
        put(c, x + 8.0, y + px * 1.9, px, th.fg, &val);
    }
    let y = 66.0 + 6.0 * rowh + 6.0;
    let sig = c.tr(
        "Signature on file. Form revision ",
        "Chữ ký đã lưu. Phiên bản biểu mẫu ",
    );
    let rev = vec![p(sig), dec("version_string", c)];
    put(c, 32.0, y, px, th.muted, &rev);
}

// ----- 4. mail client -----

fn mail_client(c: &mut Ctx) {
    let (px, th, w) = (c.px, c.th, c.w);
    c.fill(0.0, 0.0, 170.0, c.h, th.panel2);
    c.vline(170.0, 0.0, c.h, th.border);
    for (i, f) in ["Inbox", "Sent", "Drafts", "Archive", "Spam", "Trash"]
        .into_iter()
        .enumerate()
    {
        txt(
            c,
            18.0,
            18.0 + i as f32 * px * 2.2,
            px + 1.0,
            if i == 0 { th.accent } else { th.fg },
            f,
        );
    }
    // message list
    c.vline(470.0, 0.0, c.h, th.border);
    let mut y = 14.0;
    while y + px * 4.0 < c.h {
        let who = c.person();
        txt(c, 184.0, y, px + 1.0, th.fg, &who);
        let subj = vec![d("order_number", data::decoy("order_number", &mut c.rng))];
        let subj = [vec![p(c.tr("Re: order ", "Về đơn hàng "))], subj].concat();
        put(c, 184.0, y + px * 1.5, px, th.muted, &subj);
        c.hline(176.0, y + px * 3.3, 288.0, th.border);
        y += px * 4.0;
    }
    // reading pane
    let x = 490.0;
    let from = vec![p("From: "), t_or_email(c)];
    put(c, x, 16.0, px + 1.0, th.fg, &from);
    let to = vec![p("To: "), t_or_email(c)];
    put(c, x, 16.0 + px * 1.7, px, th.muted, &to);
    let when = vec![p("Date: "), dec("timestamp", c)];
    put(c, x, 16.0 + px * 3.2, px, th.muted, &when);
    c.hline(x, 16.0 + px * 4.8, w - x - 16.0, th.border);
    let hello = format!("{} {},", c.tr("Hi", "Chào"), c.person());
    txt(c, x, 16.0 + px * 6.0, px, th.fg, &hello);
    let mut y = 16.0 + px * 8.0;
    while y + px * 1.8 < c.h {
        let s = snippet(c);
        if c.measure(px, &join(&s)) > w - x - 16.0 {
            y += 1.0;
            continue;
        }
        put(c, x, y, px, th.fg, &s);
        y += px * 1.9;
    }
}

fn t_or_email(c: &mut Ctx) -> Seg {
    let v = c.value(&["email"]);
    match v.into_iter().next() {
        Some(s) if s.tag != super::scene::Tag::Plain && s.text.contains('@') => s,
        _ => p(c.person()),
    }
}

// ----- 5. chat window -----

fn chat(c: &mut Ctx) {
    let (px, th, w) = (c.px, c.th, c.w);
    c.fill(0.0, 0.0, w, 40.0, th.panel2);
    let who = c.person();
    txt(c, 16.0, 11.0, px + 3.0, th.fg, &who);
    c.hline(0.0, 40.0, w, th.border);
    let mut y = 54.0;
    let mut me = false;
    let mut guard = 0;
    while y + px * 3.4 < c.h - 8.0 && guard < 200 {
        guard += 1;
        let s = snippet(c);
        let text = join(&s);
        let tw = c.measure(px, &text);
        if tw > w * 0.78 {
            continue;
        }
        let bw = tw + 24.0;
        let bx = if me { w - bw - 16.0 } else { 16.0 };
        let col = if me { th.bubble_me } else { th.panel2 };
        c.fill(bx, y, bw, px * 2.4, col);
        put(c, bx + 12.0, y + px * 0.6, px, th.fg, &s);
        if c.rng.chance(0.3) {
            let stamp = vec![dec("timestamp", c)];
            let sx = if me { bx } else { bx + 4.0 };
            put(c, sx, y + px * 2.5, (px - 1.0).max(10.0), th.muted, &stamp);
            y += px * 1.3;
        }
        y += px * 3.1;
        me = !me;
    }
}

// ----- 6. CRM record table -----

fn crm_table(c: &mut Ctx) {
    let (px, th, w) = (c.px, c.th, c.w);
    c.fill(0.0, 0.0, w, 44.0, th.panel2);
    let title = c.tr("Customers", "Khách hàng");
    txt(c, 16.0, 12.0, px + 5.0, th.fg, title);
    let xs = [16.0, 96.0, 250.0, 500.0, 650.0, 800.0, 910.0, 1000.0];
    let heads = [
        "ID", "Name", "Email", "Phone", "IP", "Order", "Total", "Updated",
    ];
    let lh = px * 2.0;
    let mut y = 56.0;
    for (i, h) in heads.into_iter().enumerate() {
        txt(c, xs[i], y, px, th.muted, h);
    }
    y += lh;
    c.hline(0.0, y - 4.0, w, th.border);
    let mut n = 0;
    while y + lh < c.h {
        if n % 2 == 1 {
            c.fill(0.0, y - 4.0, w, lh, th.panel2);
        }
        let id = format!("C-{}", c.rng.range(10000, 99999));
        txt(c, xs[0], y, px, th.fg, &id);
        let nm = c.person();
        txt(c, xs[1], y, px, th.fg, &nm);
        let v = c.value_w(&["email"], 30);
        put(c, xs[2], y, px, th.fg, &v);
        let v = c.value(&["phone"]);
        put(c, xs[3], y, px, th.fg, &v);
        let v = c.value(&["ip_address"]);
        let v = if v.iter().any(|s| s.text.len() > 18) {
            vec![dec("version_string", c)]
        } else {
            v
        };
        put(c, xs[4], y, px, th.fg, &v);
        let o = pick_decoy(c, &["order_number", "tracking_number"]);
        put(c, xs[5], y, px, th.fg, &[o]);
        let a = dec("dollar_amount", c);
        put(c, xs[6], y, px, th.fg, &[a]);
        let u = dec("bare_date", c);
        put(c, xs[7], y, px, th.fg, &[u]);
        y += lh;
        n += 1;
    }
}

// ----- 7. HR spreadsheet -----

fn hr_sheet(c: &mut Ctx) {
    let (px, th, w) = (c.px, c.th, c.w);
    txt(c, 12.0, 8.0, px, th.muted, "fx");
    c.hline(0.0, 28.0, w, th.border);
    let heads = [
        "Employee",
        "SSN / ITIN",
        "Routing",
        "Account",
        "Hired",
        "Salary",
        "Phone",
        "Employer EIN",
    ];
    let xs = [44.0, 210.0, 380.0, 500.0, 610.0, 700.0, 800.0, 960.0];
    let rh = px * 2.0;
    let mut y = 32.0;
    c.fill(0.0, y, w, rh, th.panel2);
    for (i, x) in xs.iter().enumerate() {
        txt(c, *x, y + 3.0, px, th.fg, heads[i]);
        c.vline(*x - 6.0, 28.0, c.h, th.border);
        let col = (b'A' + i as u8) as char;
        txt(
            c,
            *x + 40.0,
            10.0 - 4.0,
            (px - 1.0).max(10.0),
            th.muted,
            &col.to_string(),
        );
    }
    y += rh;
    let mut n = 1;
    while y + rh < c.h {
        c.hline(0.0, y, w, th.border);
        txt(c, 8.0, y + 3.0, px, th.muted, &n.to_string());
        let nm = c.person();
        txt(c, xs[0], y + 3.0, px, th.fg, &nm);
        let v = c.value(&["ssn", "itin"]);
        put(c, xs[1], y + 3.0, px, th.fg, &v);
        let v = c.value(&["aba"]);
        put(c, xs[2], y + 3.0, px, th.fg, &v);
        let a = dec("masked_account", c);
        put(c, xs[3], y + 3.0, px, th.fg, &[a]);
        let a = dec("bare_date", c);
        put(c, xs[4], y + 3.0, px, th.fg, &[a]);
        let a = dec("dollar_amount", c);
        put(c, xs[5], y + 3.0, px, th.fg, &[a]);
        let v = c.value(&["phone"]);
        put(c, xs[6], y + 3.0, px, th.fg, &v);
        let v = c.value(&["ein"]);
        put(c, xs[7], y + 3.0, px, th.fg, &v);
        y += rh;
        n += 1;
    }
}

// ----- 8. terminal -----

fn terminal(c: &mut Ctx) {
    let (px, th, w, h) = (c.px, c.th, c.w, c.h);
    titlebar(c, "dev@workstation: ~/projects/app");
    c.fill(0.0, 29.0, w, h - 29.0, th.term_bg);
    let lh = px * 1.5;
    let mut y = 38.0;
    let prompt = "dev@workstation ~/app % ";
    let mut guard = 0;
    while y + lh < h - 6.0 && guard < 60 {
        guard += 1;
        let pw = c.measure(px, prompt);
        match c.rng.below(7) {
            0 => {
                put(
                    c,
                    12.0,
                    y,
                    px,
                    th.term_hi,
                    &[p(prompt), p("printenv | grep -iE 'secret|pass|token'")],
                );
                y += lh;
                for _ in 0..3 {
                    let s = c.secret_line(false);
                    put(c, 12.0, y, px, th.term_fg, &s);
                    y += lh;
                }
            }
            1 => {
                let mut segs = vec![p("Authorization: Bearer ")];
                segs.extend(c.value_w(&["token"], 70));
                let mut line = vec![p(prompt), p("curl -s -H \"")];
                line.extend(segs);
                line.push(p("\" https://api.example.com/v1/me"));
                put(c, 12.0, y, px, th.term_fg, &line);
                y += lh;
            }
            2 => {
                let mut line = vec![p(prompt), p("export AWS_ACCESS_KEY_ID=")];
                line.extend(c.value(&["token"]));
                put(c, 12.0, y, px, th.term_fg, &line);
                y += lh;
            }
            3 => {
                let mut line = vec![p(prompt), p("psql ")];
                line.extend(c.value_w(&["url_secret"], 70));
                put(c, 12.0, y, px, th.term_fg, &line);
                y += lh;
            }
            4 => {
                txt(c, 12.0, y, px, th.term_hi, prompt);
                txt(c, 12.0 + pw, y, px, th.term_fg, "ip addr show eth0");
                y += lh;
                let v = c.value(&["mac_address"]);
                let mut l = vec![p("    link/ether ")];
                l.extend(v);
                l.push(p(" brd ff:ff:ff:ff:ff:ff"));
                put(c, 12.0, y, px, th.term_fg, &l);
                y += lh;
                let v = c.value(&["ip_address"]);
                let mut l = vec![p("    inet ")];
                l.extend(v);
                l.push(p("/24 scope global eth0"));
                put(c, 12.0, y, px, th.term_fg, &l);
                y += lh;
            }
            5 => {
                txt(c, 12.0, y, px, th.term_hi, prompt);
                txt(c, 12.0 + pw, y, px, th.term_fg, "git log --oneline -3");
                y += lh;
                for _ in 0..3 {
                    let hash = d("short_hash", data::decoy("short_hash", &mut c.rng));
                    let msg = format!(
                        " {}",
                        c.rng.pick(&[
                            "fix retry loop",
                            "bump deps",
                            "add health check",
                            "tidy logs"
                        ])
                    );
                    put(c, 12.0, y, px, th.term_fg, &[hash, p(msg)]);
                    y += lh;
                }
            }
            _ => {
                let mut l = vec![p(prompt), p("ping -c1 ")];
                l.extend(c.value(&["ip_address"]));
                put(c, 12.0, y, px, th.term_fg, &l);
                y += lh;
                let ver = vec![p("tool version "), dec("version_string", c)];
                put(c, 12.0, y, px, th.term_dim, &ver);
                y += lh;
            }
        }
    }
}

// ----- 9. .env in an editor -----

fn env_file(c: &mut Ctx) {
    let (px, th, w, h) = (c.px, c.th, c.w, c.h);
    c.fill(0.0, 0.0, w, 30.0, th.panel2);
    txt(c, 14.0, 8.0, px + 1.0, th.fg, ".env");
    c.fill(0.0, 30.0, 52.0, h - 30.0, th.panel2);
    let lh = px * 1.5;
    let mut y = 38.0;
    let mut n = 1;
    let mut guard = 0;
    while y + lh < h - 6.0 && guard < 80 {
        guard += 1;
        let gutter = n.to_string();
        let kind = c.rng.below(8);
        let drawn: Option<f32> = match kind {
            0 => {
                let head = format!(
                    "# {}",
                    c.rng
                        .pick(&["Database", "Stripe", "Mail", "Storage", "Auth"])
                );
                Some(txt(c, 60.0, y, px, th.muted, &head))
            }
            1..=3 => {
                let s = c.secret_line(false);
                Some(put(c, 60.0, y, px, th.fg, &s))
            }
            4 => {
                let mut l = vec![p("DATABASE_URL=")];
                l.extend(c.value_w(&["url_secret"], 80));
                Some(put(c, 60.0, y, px, th.fg, &l))
            }
            5 => {
                let l = vec![p("APP_VERSION="), dec("version_string", c)];
                Some(put(c, 60.0, y, px, th.fg, &l))
            }
            6 => {
                let mut l = vec![p("API_TOKEN=")];
                l.extend(c.value_w(&["token"], 60));
                Some(put(c, 60.0, y, px, th.fg, &l))
            }
            _ => {
                if c.positive && c.rng.chance(0.5) && y + lh * 9.0 < h {
                    let lines = data::pem_block(&mut c.rng, 5);
                    let ny = c.block(60.0, y, px, th.fg, &lines, "private_key");
                    n += lines.len() as i32 - 1;
                    y = ny - lh;
                    Some(1.0)
                } else {
                    Some(txt(c, 60.0, y, px, th.fg, "NODE_ENV=production"))
                }
            }
        };
        if drawn.is_some() {
            txt(c, 20.0, y, px, th.muted, &gutter);
        }
        y += lh;
        n += 1;
    }
}

// ----- 10. browser developer panel -----

fn json_field(c: &mut Ctx, name: &str, kinds: &[&'static str]) -> Vec<Seg> {
    let mut v = vec![p(format!("  \"{name}\": \""))];
    v.extend(c.value_w(kinds, 44));
    v.push(p("\","));
    v
}

fn devtools(c: &mut Ctx) {
    let (px, th, w, h) = (c.px, c.th, c.w, c.h);
    c.fill(0.0, 0.0, w, 30.0, th.panel2);
    for (i, tab) in ["Elements", "Network", "Console", "Sources"]
        .into_iter()
        .enumerate()
    {
        txt(
            c,
            14.0 + i as f32 * 90.0,
            8.0,
            px + 1.0,
            if i == 1 { th.accent } else { th.fg },
            tab,
        );
    }
    c.vline(300.0, 30.0, h, th.border);
    let lh = px * 1.6;
    let mut y = 38.0;
    while y + lh < h {
        let path = format!(
            "/api/v{}/{}",
            c.rng.range(1, 3),
            c.rng.pick(&["users", "orders", "session", "export"])
        );
        txt(c, 12.0, y, px, th.fg, &path);
        let st = vec![
            p(format!("{}  ", c.rng.pick(&[200, 200, 201, 304, 401]))),
            dec("file_size", c),
        ];
        put(c, 150.0, y, px, th.muted, &st);
        y += lh;
    }
    let x = 316.0;
    let mut y = 38.0;
    txt(c, x, y, px + 1.0, th.accent, "Request Headers");
    y += lh * 1.3;
    let mut guard = 0;
    while y + lh < h * 0.5 && guard < 20 {
        guard += 1;
        let line = match c.rng.below(6) {
            0 => {
                let mut l = vec![p("authorization: Bearer ")];
                l.extend(c.value_w(&["token"], 60));
                l
            }
            1 => {
                let mut l = vec![p("x-forwarded-for: ")];
                l.extend(c.value(&["ip_address"]));
                l
            }
            2 => vec![p("x-request-id: "), dec("uuid", c)],
            3 => vec![p("etag: \""), dec("git_hash", c), p("\"")],
            4 => vec![
                p("user-agent: Mozilla/5.0 (X11; Linux x86_64) Gecko/20100101 Firefox/"),
                dec("version_string", c),
            ],
            _ => vec![p("content-type: application/json")],
        };
        put(c, x, y, px, th.fg, &line);
        y += lh;
    }
    y += lh * 0.6;
    txt(c, x, y, px + 1.0, th.accent, "Request Payload");
    y += lh * 1.3;
    put(c, x, y, px, th.fg, &[p("{")]);
    y += lh;
    let fields: [(&str, &[&'static str]); 8] = [
        ("email", &["email"]),
        ("phone", &["phone"]),
        ("ssn", &["ssn"]),
        ("card_number", &["card"]),
        ("ip", &["ip_address"]),
        ("iban", &["iban"]),
        ("vin", &["vin"]),
        ("device", &["imei", "mac_address"]),
    ];
    for (name, kinds) in fields {
        if y + lh > h - lh * 2.0 {
            break;
        }
        let l = json_field(c, name, kinds);
        put(c, x, y, px, th.fg, &l);
        y += lh;
    }
    if y + lh < h {
        let s = c.secret_line(true);
        let mut l = vec![p("  ")];
        l.extend(s);
        put(c, x, y, px, th.fg, &l);
    }
}

// ----- 11. log viewer -----

fn log_viewer(c: &mut Ctx) {
    let (px, th, w, h) = (c.px, c.th, c.w, c.h);
    titlebar(c, "logs - api-gateway");
    c.fill(0.0, 29.0, w, h - 29.0, th.term_bg);
    let lh = px * 1.5;
    let mut y = 38.0;
    let mut guard = 0;
    while y + lh < h - 4.0 && guard < 120 {
        guard += 1;
        let level = *c.rng.pick(&["INFO ", "INFO ", "WARN ", "DEBUG", "ERROR"]);
        let mut l = vec![dec("timestamp", c), p(format!(" {level} "))];
        match c.rng.below(6) {
            0 => {
                l.push(p("login ok user="));
                l.extend(c.value(&["email"]));
                l.push(p(" ip="));
                l.extend(c.value(&["ip_address"]));
            }
            1 => {
                l.push(p("req="));
                l.push(dec("uuid", c));
                l.push(p(" status=200 build="));
                l.push(dec("short_hash", c));
            }
            2 => {
                l.push(p("sms sent to "));
                l.extend(c.value(&["phone"]));
                l.push(p(" msg_id="));
                l.push(dec("tracking_number", c));
            }
            3 => {
                l.push(p("config loaded "));
                l.extend(c.secret_line(false));
            }
            4 => {
                l.push(p("upstream auth header "));
                l.extend(c.value_w(&["token"], 60));
            }
            _ => {
                l.push(p("worker "));
                l.push(dec("version_string", c));
                l.push(p(" lat="));
                l.push(dec("coordinates", c));
            }
        }
        put(c, 10.0, y, px, th.term_fg, &l);
        y += lh;
    }
}

// ----- 12. settings page -----

fn settings_page(c: &mut Ctx) {
    let (px, th, w) = (c.px, c.th, c.w);
    c.fill(0.0, 0.0, 190.0, c.h, th.panel2);
    c.vline(190.0, 0.0, c.h, th.border);
    let items = [
        ("Profile", "Hồ sơ"),
        ("Security", "Bảo mật"),
        ("Billing", "Thanh toán"),
        ("Devices", "Thiết bị"),
        ("Identity", "Danh tính"),
    ];
    for (i, (en, vi)) in items.into_iter().enumerate() {
        let s = c.tr(en, vi);
        txt(
            c,
            20.0,
            24.0 + i as f32 * px * 2.4,
            px + 1.0,
            if i == 1 { th.accent } else { th.fg },
            s,
        );
    }
    let x = 214.0;
    let lx = x + 170.0;
    let lh = px * 2.1;
    let mut y = 22.0;
    #[allow(clippy::type_complexity)]
    let sections: [(&str, &str, Vec<(&str, &str, Vec<&'static str>)>); 4] = [
        (
            "Profile",
            "Hồ sơ",
            vec![
                ("Email", "Email", vec!["email"]),
                ("Phone", "Điện thoại", vec!["phone"]),
                ("Recovery phone", "SĐT khôi phục", vec!["phone"]),
            ],
        ),
        (
            "Security",
            "Bảo mật",
            vec![
                ("API key", "Khóa API", vec!["token"]),
                ("Last login IP", "IP đăng nhập", vec!["ip_address"]),
                ("Webhook", "Webhook", vec!["url_secret"]),
            ],
        ),
        (
            "Devices",
            "Thiết bị",
            vec![
                ("IMEI", "IMEI", vec!["imei"]),
                ("Wi-Fi MAC", "MAC Wi-Fi", vec!["mac_address"]),
                ("Vehicle VIN", "Số khung (VIN)", vec!["vin"]),
            ],
        ),
        (
            "Identity",
            "Danh tính",
            vec![
                ("Passport", "Hộ chiếu", vec!["passport"]),
                ("SSN", "Số an sinh", vec!["ssn", "itin"]),
                ("Tax ID", "Mã số thuế", vec!["ein"]),
                ("Routing", "Định tuyến", vec!["aba"]),
            ],
        ),
    ];
    for (en, vi, rows) in sections {
        let heading = c.tr(en, vi);
        txt(c, x, y, px + 4.0, th.fg, heading);
        y += lh * 1.3;
        c.hline(x, y - 4.0, w - x - 20.0, th.border);
        for (len, lvi, kinds) in rows {
            let label = c.tr(len, lvi);
            txt(c, x, y, px, th.muted, label);
            let v = c.value_w(&kinds, 52);
            put(c, lx, y, px, th.fg, &v);
            y += lh;
        }
        y += lh * 0.4;
    }
    // payment method
    let bill = c.tr("Billing", "Thanh toán");
    txt(c, x, y, px + 4.0, th.fg, bill);
    y += lh * 1.3;
    let m = vec![
        p(format!("{}  ", c.tr("Saved card", "Thẻ đã lưu"))),
        dec("masked_card", c),
    ];
    put(c, lx - 170.0, y, px, th.fg, &m);
    y += lh;
    let (a, b) = c.card_two_lines();
    put(c, lx - 170.0, y, px, th.fg, &a);
    put(c, lx - 170.0, y + lh * 0.9, px, th.fg, &b);
}

// ----- 13. checkout form -----

fn checkout(c: &mut Ctx) {
    let (px, th, w) = (c.px, c.th, c.w);
    txt(c, 28.0, 20.0, px + 9.0, th.fg, "Checkout");
    let fw = w * 0.52;
    let rowh = px * 4.4;
    let mut y = 66.0;
    let fields: [(&str, &[&'static str]); 3] = [
        ("Email", &["email"]),
        ("Phone", &["phone"]),
        ("Billing address", &["@addr"]),
    ];
    for (label, kinds) in fields {
        txt(c, 28.0, y, px, th.muted, label);
        c.panel(28.0, y + px * 1.4, fw, px * 2.4);
        let v = if kinds[0] == "@addr" {
            vec![p(data::street(&mut c.rng, false))]
        } else {
            c.value(kinds)
        };
        put(c, 36.0, y + px * 1.8, px, th.fg, &v);
        y += rowh;
    }
    txt(c, 28.0, y, px, th.muted, "Payment");
    y += px * 1.6;
    c.panel(28.0, y, fw, px * 6.8);
    let (a, b) = c.card_two_lines();
    put(c, 36.0, y + px * 0.8, px + 1.0, th.fg, &a);
    put(c, 36.0, y + px * 3.0, px, th.fg, &b);
    let inl = c.card_inline();
    put(c, 36.0, y + px * 5.0, px, th.muted, &inl);
    // order summary
    let sx = fw + 60.0;
    txt(c, sx, 66.0, px + 3.0, th.fg, "Order summary");
    let o = vec![p("Order "), dec("order_number", c)];
    put(c, sx, 66.0 + px * 2.2, px, th.muted, &o);
    let mut yy = 66.0 + px * 4.6;
    let mut n = 0;
    while yy + px * 1.8 < c.h - 8.0 && n < 12 {
        let item = c.rng.pick(&data::PRODUCTS).to_string();
        txt(c, sx, yy, px, th.fg, &item);
        let a = dec("dollar_amount", c);
        put(c, sx + 170.0, yy, px, th.fg, &[a]);
        yy += px * 1.8;
        n += 1;
    }
}
