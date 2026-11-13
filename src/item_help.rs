//! O046 item-help resolution from the original PE asset and captured OS values.
//! Requests/effects are headless calculations, not native window delivery.

use crate::csv::{self, Error, history::pe_tail};
use std::borrow::Cow;

const NAMES: u32 = 0x3f2b44;
const TOPICS: u32 = 0x3db93f;
const HELP_FILES: u32 = 0x314076;
const TOPIC_RESOURCE: &[u8] = b"GI_AppItem_CellName_to_ContextID";
const FILE_RESOURCE: &[u8] = b"GI_Sys_MyHelpFile";

#[derive(Debug, Clone, Copy)]
pub enum Input<'a> {
    Item {
        key: i32,
        index: i32,
        context58: &'a [u8],
    },
    Contents,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub key: Option<i32>,
    pub index: Option<i32>,
    pub group: Option<i32>,
    pub original_name: Vec<u8>,
    pub name: Vec<u8>,
    pub topic: i32,
    pub chm: Vec<u8>,
    pub target: Vec<u8>,
}

fn c_string(bytes: &[u8]) -> Result<(&[u8], &[u8]), Error> {
    let end = bytes
        .iter()
        .take(65537)
        .position(|b| *b == 0)
        .ok_or(Error::SourceUnavailable(
            "item-help bounded source NUL unavailable".into(),
        ))?;
    Ok((&bytes[..end], &bytes[end + 1..]))
}

fn named<'a>(pe: &'a [u8], rva: u32, name: &[u8]) -> Result<&'a [u8], Error> {
    let (actual, data) = c_string(pe_tail(pe, rva)?)?;
    if actual != name {
        return Err(Error::SourceUnavailable(
            "item-help named resource unavailable".into(),
        ));
    }
    Ok(data)
}

fn key_name(pe: &[u8], key: i32) -> Result<Vec<u8>, Error> {
    if !(0..=0x3ad).contains(&key) {
        return Ok(Vec::new());
    }
    let pointer = pe_tail(pe, NAMES + 4 * key as u32)?
        .get(..4)
        .ok_or(Error::SourceUnavailable(
            "item-help key pointer unavailable".into(),
        ))?;
    let rva = u32::from_le_bytes(pointer.try_into().unwrap())
        .checked_sub(0x400000)
        .ok_or(Error::SourceUnavailable(
            "item-help key pointer outside image".into(),
        ))?;
    Ok(c_string(pe_tail(pe, rva)?)?.0.to_vec())
}

fn group(context: &[u8]) -> Result<i32, Error> {
    let token = csv::trim(context.split(|b| *b == b',').next().unwrap());
    if token.is_empty() {
        Ok(-1)
    } else {
        csv::number::int32(token)
    }
}

fn alias(name: &[u8], group: i32, index: i32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut rest = name;
    if matches!(group, 0x191 | 0x192) {
        while let Some(at) = rest.windows(7).position(|s| s == b"CL_Mch_") {
            out.extend_from_slice(&rest[..at]);
            out.extend_from_slice(b"CL_BsrMPad_");
            rest = &rest[at + 7..];
        }
    }
    out.extend_from_slice(rest);
    if out == b"CL_Common_Notch_Q" && (0..4).contains(&index) {
        out.extend_from_slice([b"_SSB".as_slice(), b"_CW", b"_RTTY", b"_AM"][index as usize]);
    }
    out
}

fn decimal_topic(text: &[u8]) -> Result<i32, Error> {
    let text = csv::trim(text);
    let sign = usize::from(matches!(text.first(), Some(b'+' | b'-')));
    let digits = text[sign..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count();
    if digits == 0 {
        return Ok(0);
    }
    csv::number::int32(&text[..sign + digits])
}

fn topic(mut data: &[u8], name: &[u8]) -> Result<i32, Error> {
    let start = data.len();
    for _ in 0..4096 {
        let (line, tail) = c_string(data)?;
        if start - tail.len() > 65536 {
            return Err(Error::Limit("item-help resource size".into()));
        }
        data = tail;
        if line == b"#" {
            return Ok(0);
        }
        let mut tokens = line.split(|b| *b == b'\t');
        if tokens.next() == Some(name) {
            return decimal_topic(tokens.next().unwrap_or_default());
        }
    }
    Err(Error::Limit("item-help resource row count".into()))
}

/// The original asset supplies key names, topic rows and the two CHM filenames.
/// Application directory bytes and GUI C+0 are actual external state, not locale.
pub fn resolve(
    pe: &[u8],
    input: Input<'_>,
    gui_language: u32,
    application_dir: &[u8],
) -> Result<Request, Error> {
    if application_dir.contains(&0) {
        return Err(Error::Invalid(
            "application directory must be a native NUL-free path".into(),
        ));
    }
    let (key, index, group, original_name, name, number) = match input {
        Input::Contents => (None, None, None, Vec::new(), Vec::new(), -1),
        Input::Item {
            key,
            index,
            context58,
        } => {
            let original = key_name(pe, key)?;
            let group = group(context58)?;
            let name = alias(&original, group, index);
            let number = topic(named(pe, TOPICS, TOPIC_RESOURCE)?, &name)?;
            (Some(key), Some(index), Some(group), original, name, number)
        }
    };
    let files = named(pe, HELP_FILES, FILE_RESOURCE)?;
    let (english, rest) = c_string(files)?;
    let filename = if gui_language == 1 {
        c_string(rest)?.0
    } else {
        english
    };
    if filename == b"#" {
        return Err(Error::SourceUnavailable(
            "item-help CHM resource row unavailable".into(),
        ));
    }
    let mut chm = application_dir.to_vec();
    chm.extend_from_slice(filename);
    let mut target = chm.clone();
    if number != -1 {
        target.extend_from_slice(b">IcomPopupWindow");
    }
    Ok(Request {
        key,
        index,
        group,
        original_name,
        name,
        topic: number,
        chm,
        target,
    })
}

/// Actual helper cache and two independent document window members.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    pub module: u32,
    pub procedure: u32,
    pub cookie: u32,
    pub d188: u32,
    pub d184: u32,
}

/// None means an observation was not supplied; Some(0) is a real API zero.
#[derive(Debug, Clone, Default)]
pub struct Api {
    pub load_library: Option<u32>,
    pub get_proc_address: Option<u32>,
    pub initialize_cookie: Option<u32>,
    pub window_before: Option<u32>,
    pub old_rect: Option<[i32; 4]>,
    pub display_window: Option<u32>,
    pub file_attributes: Option<u32>,
    pub window_after: Option<u32>,
    pub window_info_pointer: Option<u32>,
    pub window_info_rect: Option<[i32; 4]>,
    pub cursor: Option<[i32; 2]>,
    pub work_area: Option<[i32; 4]>,
    pub style: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// Native observations borrow literals; callers may own arbitrary API names.
    pub api: Cow<'static, str>,
    pub command: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub window: u32,
    pub x: i32,
    pub y: i32,
    pub style: u32,
    pub flags: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Effects {
    pub calls: Vec<Call>,
    pub show_window: Vec<(u32, i32)>,
    pub placement: Option<Placement>,
    pub display_window_observed: Option<u32>,
    pub message_resource_index: Option<u32>,
    pub message_icon: Option<u32>,
}
impl Effects {
    fn call(&mut self, api: &'static str, command: Option<u32>) {
        self.calls.push(Call {
            api: api.into(),
            command,
        });
    }
}
fn observed<T: Copy>(value: Option<T>, name: &'static str) -> Result<T, Error> {
    value.ok_or(Error::SourceUnavailable(name.into()))
}

fn position(api: &Api, rect: [i32; 4], effects: &mut Effects) -> Result<(), Error> {
    effects.call("HtmlHelpA", Some(6));
    let window = observed(api.window_after, "HtmlHelpA position window output")?;
    if window == 0 {
        return Ok(());
    }
    let rect = if rect == [0; 4] {
        effects.call("HtmlHelpA", Some(5));
        if observed(
            api.window_info_pointer,
            "HtmlHelpA window-info pointer output",
        )? == 0
        {
            return Err(Error::Operation(
                "native HtmlHelp window-info NULL dereference".into(),
            ));
        }
        observed(
            api.window_info_rect,
            "HtmlHelpA window-info raw RECT source",
        )?
    } else {
        rect
    };
    effects.call("GetCursorPos", None);
    let [cx, cy] = observed(api.cursor, "GetCursorPos raw POINT output")?;
    let [left, top, right, bottom] =
        observed(api.work_area, "native monitor work-area RECT output")?;
    let width = rect[2].wrapping_sub(rect[0]);
    let height = rect[3].wrapping_sub(rect[1]);
    let mut x = if left.wrapping_add(width / 2) < cx {
        cx.wrapping_sub(width / 2)
    } else {
        left
    };
    let mut y = cy;
    if cy.wrapping_add(height) > bottom {
        y = if height < cy.wrapping_sub(top) && cy < bottom {
            cy.wrapping_sub(height)
        } else {
            bottom.wrapping_sub(height)
        };
    }
    if x.wrapping_add(width) > right {
        x = right.wrapping_sub(width);
    }
    effects.call("GetWindowLongA", None);
    let style = observed(api.style, "GetWindowLongA raw style output")? & !0x30000;
    effects.call("SetWindowLongA", None);
    effects.call("SetWindowPos", None);
    effects.placement = Some(Placement {
        window,
        x,
        y,
        style,
        flags: 0x41,
    });
    Ok(())
}

/// Replay the reached OS observation branches only. This never starts Windows.
/// On error the state/effects retain the real attempted prefix, including NULL.
pub fn drive(
    request: &Request,
    state: &mut State,
    api: &Api,
    effects: &mut Effects,
) -> Result<u32, Error> {
    *effects = Effects::default();
    if state.module == 0 && state.procedure != 0 {
        return Err(Error::Invalid(
            "inconsistent native module/procedure snapshot".into(),
        ));
    }
    if state.module == 0 {
        effects.call("LoadLibraryA(HHCTRL.OCX)", None);
        state.module = observed(api.load_library, "LoadLibraryA output")?;
        if state.module != 0 {
            effects.call("GetProcAddress(HtmlHelpA)", None);
            state.procedure = observed(api.get_proc_address, "GetProcAddress output")?;
            if state.procedure != 0 {
                effects.call("HtmlHelpA", Some(0x1c));
                state.cookie = observed(api.initialize_cookie, "HH_INITIALIZE cookie output")?;
            } else {
                effects.call("FreeLibrary", None);
                state.module = 0;
            }
        } else {
            effects.call("FreeLibrary", None);
        }
    }
    if state.procedure == 0 {
        return Ok(3);
    }
    if request.topic == -1 {
        effects.call("HtmlHelpA", Some(1));
        let window = observed(api.display_window, "HtmlHelpA contents HWND output")?;
        effects.display_window_observed = Some(window);
        state.d184 = window;
        let status = u32::from(window == 0);
        message(status, effects);
        return Ok(status);
    }
    effects.call("HtmlHelpA", Some(6));
    let old = observed(api.window_before, "HtmlHelpA popup HWND output")?;
    let rect = if old == 0 {
        effects.call("HtmlHelpA", Some(4));
        [0; 4]
    } else {
        effects.call("GetWindowRect", None);
        let rect = observed(api.old_rect, "GetWindowRect raw RECT output")?;
        effects.call("ShowWindow", Some(0));
        effects.show_window.push((old, 0));
        rect
    };
    effects.call("HtmlHelpA", Some(0x0f));
    let window = observed(api.display_window, "HtmlHelpA context HWND output")?;
    effects.display_window_observed = Some(window);
    let status = if window != 0 {
        position(api, rect, effects)?;
        0
    } else {
        effects.call("GetFileAttributesA", None);
        let attributes = observed(api.file_attributes, "GetFileAttributesA output")?;
        if attributes == u32::MAX || attributes & 0x10 != 0 {
            1
        } else {
            effects.call("ShowWindow", Some(4));
            effects.show_window.push((old, 4));
            2
        }
    };
    state.d188 = window;
    message(status, effects);
    Ok(status)
}

fn message(status: u32, effects: &mut Effects) {
    if matches!(status, 1 | 2) {
        effects.message_resource_index = Some(status - 1);
        effects.message_icon = Some(if status == 1 { 0x10 } else { 0x30 });
        effects.call("MessageBoxW", None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put32(pe: &mut [u8], at: usize, value: u32) {
        pe[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn source(name: &[u8], rows: &[u8]) -> Vec<u8> {
        // Four small independent file-backed sections, no original table copy.
        let mut pe = vec![0; 0x9400];
        pe[..2].copy_from_slice(b"MZ");
        put32(&mut pe, 0x3c, 0x80);
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        pe[0x84..0x86].copy_from_slice(&0x14c_u16.to_le_bytes());
        pe[0x86..0x88].copy_from_slice(&4_u16.to_le_bytes());
        pe[0x94..0x96].copy_from_slice(&0xe0_u16.to_le_bytes());
        pe[0x98..0x9a].copy_from_slice(&0x10b_u16.to_le_bytes());
        put32(&mut pe, 0xb4, 0x400000);
        for (i, (rva, size, raw)) in [
            (0x314000, 0x1000, 0x400),
            (0x3db000, 0x5000, 0x1400),
            (0x3f2000, 0x2000, 0x6400),
            (0x410000, 0x1000, 0x8400),
        ]
        .into_iter()
        .enumerate()
        {
            let at = 0x178 + 40 * i;
            put32(&mut pe, at + 8, size);
            put32(&mut pe, at + 12, rva);
            put32(&mut pe, at + 16, size);
            put32(&mut pe, at + 20, raw);
        }
        put32(&mut pe, 0x6f44, 0x810000); // key 0, not a completed field name input.
        pe[0x8400..0x8400 + name.len()].copy_from_slice(name);
        let files = [FILE_RESOURCE, b"\0english.chm\0japanese.chm\0\0#\0"].concat();
        pe[0x476..0x476 + files.len()].copy_from_slice(&files);
        let rows = [TOPIC_RESOURCE, b"\0", rows].concat();
        pe[0x1d3f..0x1d3f + rows.len()].copy_from_slice(&rows);
        pe
    }

    #[test]
    fn bounded_pe_source_resolves_names_topics_and_gui_not_host_chm() {
        let pe = source(
            b"CL_Mch_Freq",
            b"CL_BsrMPad_Freq\t321.htm\0CL_Mch_Freq\t123.htm\0#\0",
        );
        let input = Input::Item {
            key: 0,
            index: 0,
            context58: b"401,anything",
        };
        let request = resolve(&pe, input, 2, b"C:\\\x82\xa0\\").unwrap();
        assert_eq!(request.original_name, b"CL_Mch_Freq");
        assert_eq!(request.name, b"CL_BsrMPad_Freq");
        assert_eq!(request.topic, 321);
        assert_eq!(request.chm, b"C:\\\x82\xa0\\english.chm");
        assert!(request.target.ends_with(b">IcomPopupWindow"));
        assert_eq!(
            resolve(&pe, input, 1, b"C:\\").unwrap().chm,
            b"C:\\japanese.chm"
        );
        for key in [-1, 0x3ae, i32::MAX] {
            let request = resolve(
                &pe,
                Input::Item {
                    key,
                    index: 0,
                    context58: b"",
                },
                0,
                b"",
            )
            .unwrap();
            assert!(request.name.is_empty());
            assert_eq!(request.topic, 0);
        }
        let contents = resolve(&pe, Input::Contents, 0, b"C:\\").unwrap();
        assert_eq!(contents.topic, -1);
        assert_eq!(contents.target, contents.chm);
        assert!(resolve(&pe, input, 0, b"C:\0bad").is_err());
    }

    #[test]
    fn source_failure_is_not_topic_zero_and_all_four_aliases_use_the_resource() {
        let pe = source(b"CL_Common_Notch_Q", b"CL_Common_Notch_Q_SSB\t10.htm\0CL_Common_Notch_Q_CW\t11.htm\0CL_Common_Notch_Q_RTTY\t12.htm\0CL_Common_Notch_Q_AM\t13.htm\0#\0");
        for index in 0..4 {
            assert_eq!(
                resolve(
                    &pe,
                    Input::Item {
                        key: 0,
                        index,
                        context58: b""
                    },
                    0,
                    b""
                )
                .unwrap()
                .topic,
                10 + index
            );
        }
        assert_eq!(
            resolve(
                &pe,
                Input::Item {
                    key: 0,
                    index: 4,
                    context58: b""
                },
                0,
                b""
            )
            .unwrap()
            .topic,
            0
        );
        for len in [0, 63, 0x97, 0x476] {
            assert!(resolve(&pe[..len], Input::Contents, 0, b"").is_err());
        }
        assert!(resolve(&pe[..0x1400], Input::Contents, 0, b"").is_ok());
        let mut bad = pe.clone();
        bad[0x1d3f] = b'!';
        assert!(
            resolve(
                &bad,
                Input::Item {
                    key: 0,
                    index: 0,
                    context58: b""
                },
                0,
                b""
            )
            .is_err()
        );
        let mut bad = pe.clone();
        put32(&mut bad, 0x6f44, 0x3fffff);
        assert!(
            resolve(
                &bad,
                Input::Item {
                    key: 0,
                    index: 0,
                    context58: b""
                },
                0,
                b""
            )
            .is_err()
        );
        let mut bad = pe;
        bad[0x8400..].fill(b'X');
        assert!(
            resolve(
                &bad,
                Input::Item {
                    key: 0,
                    index: 0,
                    context58: b""
                },
                0,
                b""
            )
            .is_err()
        );
    }

    #[test]
    #[ignore = "requires CS705_ITEM_HELP_ASSET pointing to the approved original PE"]
    fn fixed_original_asset_few_topics_without_copying_the_table() {
        let path = std::env::var("CS705_ITEM_HELP_ASSET").expect("explicit original asset source");
        let pe = std::fs::read(path).unwrap();
        for (key, name, topic) in [
            (0x136, b"CL_My_Callsign_Code".as_slice(), 55782),
            (0x1bc, b"CL_Common_GPS_Select", 55081),
        ] {
            let result = resolve(
                &pe,
                Input::Item {
                    key,
                    index: 0,
                    context58: b"",
                },
                0,
                b"C:\\app\\",
            )
            .unwrap();
            assert_eq!((result.name.as_slice(), result.topic), (name, topic));
            assert_eq!(result.chm, b"C:\\app\\CS705_ENG.chm");
        }
    }

    fn state() -> State {
        State {
            module: 0,
            procedure: 0,
            cookie: 19,
            d188: 77,
            d184: 88,
        }
    }
    fn request(topic: i32) -> Request {
        Request {
            key: Some(0),
            index: Some(0),
            group: Some(-1),
            original_name: Vec::new(),
            name: Vec::new(),
            topic,
            chm: b"C:\\app\\help.chm".to_vec(),
            target: b"C:\\app\\help.chm>IcomPopupWindow".to_vec(),
        }
    }
    fn ready() -> Api {
        Api {
            load_library: Some(11),
            get_proc_address: Some(22),
            initialize_cookie: Some(33),
            window_before: Some(0),
            display_window: Some(44),
            window_after: Some(44),
            window_info_pointer: Some(55),
            window_info_rect: Some([0, 0, 200, 100]),
            cursor: Some([900, 750]),
            work_area: Some([0, 0, 1000, 800]),
            style: Some(u32::MAX),
            ..Api::default()
        }
    }

    #[test]
    fn group_alias_and_first_case_sensitive_tab_topic_follow_native_rules() {
        assert_eq!(group(b" 401 ,ignored").unwrap(), 401);
        assert_eq!(group(b" ,ignored").unwrap(), -1);
        assert!(group(b"not-a-number,401").is_err());
        for g in [401, 402] {
            assert_eq!(alias(b"CL_Mch_Freq", g, 0), b"CL_BsrMPad_Freq");
        }
        assert_eq!(alias(b"CL_Mch_Freq", 400, 0), b"CL_Mch_Freq");
        for (i, suffix) in [b"SSB".as_slice(), b"CW", b"RTTY", b"AM"]
            .iter()
            .enumerate()
        {
            assert_eq!(
                alias(b"CL_Common_Notch_Q", -1, i as i32),
                [b"CL_Common_Notch_Q_".as_slice(), suffix].concat()
            );
        }
        assert_eq!(alias(b"CL_Common_Notch_Q", -1, 4), b"CL_Common_Notch_Q");
        assert_eq!(
            topic(b"Key\t123.htm\0Key\t999.htm\0#\0", b"Key").unwrap(),
            123
        );
        assert_eq!(
            topic(b"Key\t123.htm\0#\0key\t999.htm\0", b"key").unwrap(),
            0
        );
        assert!(topic(b"Key\t123.htm", b"Key").is_err());
        assert_eq!(decimal_topic(b" -12.htm").unwrap(), -12);
        assert_eq!(decimal_topic(b"missing").unwrap(), 0);
    }

    #[test]
    fn api_statuses_cache_prefix_and_null_position_are_not_success_fallbacks() {
        let mut s = state();
        let mut e = Effects::default();
        assert_eq!(
            drive(
                &request(123),
                &mut s,
                &Api {
                    load_library: Some(0),
                    ..Api::default()
                },
                &mut e
            )
            .unwrap(),
            3
        );
        assert_eq!(s.d188, 77);
        assert!(e.message_icon.is_none());
        let mut s = state();
        assert_eq!(
            drive(
                &request(123),
                &mut s,
                &Api {
                    load_library: Some(11),
                    get_proc_address: Some(0),
                    ..Api::default()
                },
                &mut e
            )
            .unwrap(),
            3
        );
        assert_eq!((s.module, s.d188), (0, 77));
        for (attributes, status, icon) in [(u32::MAX, 1, 0x10), (0x10, 1, 0x10), (0x20, 2, 0x30)] {
            let mut s = state();
            let mut api = ready();
            api.window_before = Some(9);
            api.old_rect = Some([1, 2, 201, 102]);
            api.display_window = Some(0);
            api.file_attributes = Some(attributes);
            assert_eq!(drive(&request(123), &mut s, &api, &mut e).unwrap(), status);
            assert_eq!(s.d188, 0);
            assert_eq!(e.message_icon, Some(icon));
            assert_eq!(
                e.show_window,
                if status == 2 {
                    vec![(9, 0), (9, 4)]
                } else {
                    vec![(9, 0)]
                }
            );
        }
        let mut s = state();
        assert_eq!(drive(&request(123), &mut s, &ready(), &mut e).unwrap(), 0);
        assert_eq!((s.cookie, s.d188), (33, 44));
        assert_eq!(
            e.placement.unwrap(),
            Placement {
                window: 44,
                x: 800,
                y: 650,
                style: 0xfffcffff,
                flags: 0x41
            }
        );
        assert!(e.message_icon.is_none());
        let mut s = State {
            module: 11,
            procedure: 22,
            ..state()
        };
        let api = Api {
            window_before: Some(9),
            old_rect: Some([1, 2, 401, 302]),
            display_window: Some(44),
            window_after: Some(44),
            cursor: Some([900, 750]),
            work_area: Some([0, 0, 1000, 800]),
            style: Some(u32::MAX),
            ..Api::default()
        };
        assert_eq!(drive(&request(123), &mut s, &api, &mut e).unwrap(), 0);
        assert_eq!((s.cookie, s.d188), (19, 44));
        assert_eq!(
            e.placement.unwrap(),
            Placement {
                window: 44,
                x: 600,
                y: 450,
                style: 0xfffcffff,
                flags: 0x41
            }
        );
        assert!(
            !e.calls
                .iter()
                .any(|call| { call.command == Some(5) || call.api.starts_with("LoadLibrary") })
        );
        let mut s = state();
        let mut api = ready();
        api.window_info_pointer = Some(0);
        assert!(matches!(
            drive(&request(123), &mut s, &api, &mut e),
            Err(Error::Operation(_))
        ));
        assert_eq!((s.module, s.procedure, s.cookie, s.d188), (11, 22, 33, 77));
        assert_eq!(e.display_window_observed, Some(44));
        let mut s = state();
        assert!(
            drive(
                &request(123),
                &mut s,
                &Api {
                    load_library: Some(11),
                    ..Api::default()
                },
                &mut e
            )
            .is_err()
        );
        assert_eq!((s.module, s.d188), (11, 77));
    }

    #[test]
    fn contents_has_its_own_window_and_no_popup_attribute_or_position_path() {
        let mut s = State {
            module: 11,
            procedure: 22,
            ..state()
        };
        let mut e = Effects::default();
        assert_eq!(
            drive(
                &request(-1),
                &mut s,
                &Api {
                    display_window: Some(0),
                    ..Api::default()
                },
                &mut e
            )
            .unwrap(),
            1
        );
        assert_eq!((s.d184, s.d188), (0, 77));
        assert_eq!(
            e.calls.iter().map(|c| c.command).collect::<Vec<_>>(),
            [Some(1), None]
        );
    }
}
