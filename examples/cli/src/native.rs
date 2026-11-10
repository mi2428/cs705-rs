//! Explicit generated-B and live-Q inputs shared by offline native operations.
//! Callers validate the complete stdin ownership set before reading this file.

use crate::{AppError, Result, io};
use cs705::icf;
use serde_json::{Value, json};

pub struct Context {
    pub ranges: icf::frequency::Ranges,
    pub live_q: icf::context::Context,
    pub system_langid: Option<u16>,
}

pub fn read(path: &str) -> Result<Context> {
    let (generation, live_q, system_langid) = parse_sources(&io::read(path)?)?;
    Ok(Context {
        ranges: icf::frequency::Ranges::generate(&generation, system_langid)?,
        live_q,
        system_langid,
    })
}

/// Read validated live Q and optional SYSTEM input without generating B.
pub fn read_live(path: &str) -> Result<(icf::context::Context, Option<u16>)> {
    let (_, live_q, system_langid) = parse_sources(&io::read(path)?)?;
    Ok((live_q, system_langid))
}

/// Read validated generation Q without resolving SYSTEM input or generating B.
pub fn read_generation(path: &str) -> Result<icf::context::Context> {
    let (generation, _, _) = parse_sources(&io::read(path)?)?;
    Ok(generation)
}

fn parse_sources(
    bytes: &[u8],
) -> Result<(icf::context::Context, icf::context::Context, Option<u16>)> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| AppError::invalid("invalid native range context JSON"))?;
    let object = value
        .as_object()
        .ok_or_else(|| AppError::invalid("native range context must be an object"))?;
    if !object.contains_key("generation_q_hex")
        || !object.contains_key("live_q_hex")
        || object.keys().any(|k| {
            !matches!(
                k.as_str(),
                "generation_q_hex" | "live_q_hex" | "system_langid"
            )
        })
    {
        return Err(AppError::invalid(
            "range context requires generation_q_hex/live_q_hex and optional system_langid only",
        ));
    }
    let q = |key: &str| -> Result<icf::context::Context> {
        let hex = value[key].as_str().ok_or_else(|| {
            AppError::invalid("Q context requires an explicit hexadecimal string")
        })?;
        let raw: [u8; 24] = io::decode_hex(hex)?
            .try_into()
            .map_err(|_| AppError::invalid("Q context requires exactly24 bytes"))?;
        Ok(icf::context::Context::from_bytes(raw))
    };
    let generation = q("generation_q_hex")?;
    let live_q = q("live_q_hex")?;
    let system_langid = object
        .get("system_langid")
        .filter(|v| !v.is_null())
        .map(|v| {
            v.as_u64()
                .and_then(|n| u16::try_from(n).ok())
                .ok_or_else(|| AppError::invalid("SYSTEM LANGID requires an explicit u16"))
        })
        .transpose()?;
    Ok((generation, live_q, system_langid))
}

pub fn report(context: &Context) -> Value {
    json!({"selected":context.ranges.selected(),"generation_q":context.ranges.source_q().map(|q|io::bytes_value(q)),"live_q":io::bytes_value(context.live_q.as_bytes()),"system_langid":context.system_langid,"automatic_refresh":false})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources() -> Value {
        json!({
            "generation_q_hex": "100000000101000003000000010100010100010001000000",
            "live_q_hex": "FFFFFFFFA5FE123456789ABCDEF00102030405060708090A"
        })
    }

    #[test]
    fn r3_native_q_sources_do_not_require_system_or_repair_unknown_bytes() {
        let value = sources();
        let (generation, live_q, system) =
            parse_sources(&serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(generation.selector().unwrap(), 16);
        assert_eq!(system, None);
        assert_eq!(
            live_q.as_bytes().as_slice(),
            io::decode_hex(value["live_q_hex"].as_str().unwrap()).unwrap()
        );
        // Only the existing full read generates B and needs SYSTEM here.
        assert!(icf::frequency::Ranges::generate(&generation, system).is_err());
        for langid in [Value::Null, json!(0), json!(65535)] {
            let mut value = value.clone();
            value["system_langid"] = langid.clone();
            let (other_generation, other_live, system) =
                parse_sources(&serde_json::to_vec(&value).unwrap()).unwrap();
            assert_eq!(other_generation, generation);
            assert_eq!(other_live, live_q);
            assert_eq!(system, langid.as_u64().map(|n| n as u16));
        }
    }

    #[test]
    fn r3_native_q_sources_validate_both_q_values_and_all_members() {
        for key in ["generation_q_hex", "live_q_hex"] {
            let mut missing = sources();
            missing.as_object_mut().unwrap().remove(key);
            assert!(parse_sources(&serde_json::to_vec(&missing).unwrap()).is_err());
            for invalid in [Value::Null, json!(0), json!("00"), json!("GG".repeat(24))] {
                let mut value = sources();
                value[key] = invalid;
                assert!(parse_sources(&serde_json::to_vec(&value).unwrap()).is_err());
            }
        }
        for invalid in [
            json!(-1),
            json!(65536),
            json!(1.5),
            json!("1033"),
            json!(true),
        ] {
            let mut value = sources();
            value["system_langid"] = invalid;
            assert!(parse_sources(&serde_json::to_vec(&value).unwrap()).is_err());
        }
        let mut unknown = sources();
        unknown["extra"] = Value::Null;
        assert!(parse_sources(&serde_json::to_vec(&unknown).unwrap()).is_err());
        for invalid in [b"{".as_slice(), b"[]", b"null"] {
            assert!(parse_sources(invalid).is_err());
        }
    }
}
