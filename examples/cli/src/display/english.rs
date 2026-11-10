//! Independent English presentation of the existing semantic field schema.
//! Labels are derived from public identifiers, never from application resources.

use crate::{AppError, Result, fields, records};
use cs705::icf::{Icf, settings};
use serde_json::{Value, json};

pub fn label(key: &str) -> String {
    key.replace(['/', '-', '_'], " ")
}

pub fn catalog() -> Value {
    let mut catalog = fields::list();
    for field in catalog.as_array_mut().expect("compiled field catalog") {
        field["label"] = json!(label(field["key"].as_str().expect("compiled field path")));
        if let Some(packed) = settings::extended::PackedField::ALL
            .into_iter()
            .find(|p| field["key"] == format!("choice/{}/0", p.feature_id()))
        {
            use settings::extended::PackedField;
            // These packed operations do not export a Setting descriptor.
            let max = match packed {
                PackedField::SelectMemoryScan | PackedField::ScopeMode => 3,
                PackedField::DeltaFSpan => 6,
                _ => 1,
            };
            field["menu_domain"] = json!({"min":0,"max":max,"step":1});
        }
    }
    catalog
}

fn matches_path(pattern: &str, key: &str) -> bool {
    let pattern: Vec<_> = pattern.split('/').collect();
    let key: Vec<_> = key.split('/').collect();
    pattern.len() == key.len()
        && pattern.iter().zip(key).all(|(p, k)| {
            *p == k
                || p.starts_with('<') && p.ends_with('>')
                || matches!(*p, "INDEX" | "GROUP") && k.parse::<usize>().is_ok()
        })
}

fn matches_field(field: &Value, key: &str) -> bool {
    let Some(pattern) = field["key"].as_str() else {
        return false;
    };
    if matches_path(pattern, key) {
        return true;
    }
    if let (Some((parent, _)), Some((actual, index))) =
        (pattern.rsplit_once('/'), key.rsplit_once('/'))
        && parent == actual
        && let Ok(index) = index.parse::<usize>()
    {
        return field["indices"]
            .as_array()
            .is_some_and(|a| a.contains(&json!(index)))
            || field["count"].as_u64().is_some_and(|n| index < n as usize);
    }
    key.strip_prefix(pattern)
        .and_then(|s| s.strip_prefix('/'))
        .is_some_and(|suffix| {
            field["arguments"]
                .as_array()
                .is_some_and(|a| !a.is_empty() && a.len() == suffix.split('/').count())
        })
}

pub fn describe(key: &str) -> Result<Value> {
    let catalog = catalog();
    let fields = catalog.as_array().expect("compiled field catalog");
    let exact: Vec<_> = fields.iter().filter(|f| f["key"] == key).collect();
    let matches: Vec<_> = if exact.is_empty() {
        fields
            .iter()
            .filter(|f| f["feature_id"] == key || matches_field(f, key))
            .collect()
    } else {
        exact
    };
    if matches.len() != 1 {
        return Err(AppError::invalid(
            "field help requires one catalog path or unambiguous feature ID",
        ));
    }
    Ok(matches[0].clone())
}

/// Schema domains are data. A numeric code is never guessed to mean a caption.
pub fn choices(key: &str, image: Option<&Icf<'_>>) -> Result<Value> {
    let field = describe(key)?;
    let key = if field["feature_id"] == key {
        field["key"].as_str().expect("compiled field path")
    } else {
        key
    };
    let domain = field.get("menu_domain").or_else(|| field.get("value"));
    let mut current = None;
    let mut actual = None;
    if let Some(path) = key.strip_prefix("setting/") {
        let (name, index) = path
            .rsplit_once('/')
            .ok_or_else(|| AppError::invalid("setting choices need NAME/INDEX"))?;
        let index: usize = crate::values::parse(index)?;
        let setting = settings::find(name)?;
        if setting.kind == settings::Kind::Selector {
            if let Some(image) = image {
                actual = Some(setting.choices(image, index)?);
                let settings::Value::Number(value) = setting.read(image, index)?;
                current = Some(value);
            } else if matches!(setting.key, 0x2fc | 0x2fd) {
                return Err(AppError::invalid(
                    "USB assignment choices require an input image",
                ));
            }
        }
    }
    if let Some(path) = key.strip_prefix("choice/") {
        let (name, index) = path
            .rsplit_once('/')
            .ok_or_else(|| AppError::invalid("choice listing needs NAME/INDEX"))?;
        let index = crate::values::parse::<usize>(index)?;
        if let Some(choice) = settings::extended::choice_fields()
            .iter()
            .chain(settings::extended::rf_choice_fields())
            .find(|f| (f.name() == name || f.feature_id() == name) && f.accepts_index(index))
        {
            if choice.fixed_value(index)?.is_some() {
                actual = Some(Vec::new());
            }
            if let Some(image) = image {
                current = Some(choice.read(image, index)?);
            }
        }
    }
    let codes = |values: Vec<i64>| {
        values
            .into_iter()
            .map(|value| json!({"value":value,"label":format!("Code {value}")}))
            .collect::<Vec<_>>()
    };
    let candidates = if let Some(actual) = actual {
        codes(actual)
    } else if let Some(names) = domain.and_then(Value::as_array) {
        names.iter().map(|v| json!({"value":v,"label":v})).collect()
    } else if domain.is_some_and(|v| v["type"] == "boolean") {
        vec![
            json!({"value":false,"label":"Disabled"}),
            json!({"value":true,"label":"Enabled"}),
        ]
    } else if let Some((low, high)) = domain.and_then(|v| v["min"].as_i64().zip(v["max"].as_i64()))
    {
        let step = domain.and_then(|v| v["step"].as_i64()).unwrap_or(1);
        if step <= 0 || high < low {
            return Err(AppError::invalid("invalid field choice domain"));
        }
        // Large numeric domains are input ranges, not lists of menu entries.
        if i128::from(high) - i128::from(low) > 4095 * i128::from(step) {
            Vec::new()
        } else {
            (low..=high)
                .step_by(step as usize)
                .map(|value| json!({"value":value,"label":format!("Code {value}")}))
                .collect()
        }
    } else {
        Vec::new()
    };
    Ok(
        json!({"field":field,"candidates":candidates,"domain":domain,"current":current,
        "note":"Codes and ranges come from the semantic schema. Unlisted stored values are not replaced."}),
    )
}

pub fn read(image: &Icf<'_>, keys: &[String], runtime: &records::Runtime) -> Result<Value> {
    keys.iter()
        .map(|key| {
            let mut value = if key.starts_with("record/repeater/") {
                records::repeater_get(image, key, runtime)?
            } else if ["record/from-dv/", "record/from-fm/", "record/to/"]
                .iter()
                .any(|p| key.starts_with(p))
            {
                records::history_get(image, key, runtime)?
            } else {
                fields::get(image, key)?
            };
            if let Some(object) = value.as_object_mut()
                && object.remove("fixed_label").is_some_and(|v| !v.is_null())
            {
                object.insert(
                    "read_only_reason".into(),
                    json!("This mode has a fixed value instead of an editable choice."),
                );
            }
            Ok(json!({"key":key,"label":label(key),"value":value}))
        })
        .collect::<Result<Vec<_>>>()
        .map(Value::Array)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_labels_and_choices_use_schema_without_resources() {
        assert_eq!(
            label("setting/beep_level_raw/0"),
            "setting beep level raw 0"
        );
        assert!(matches_path("record/call/INDEX/name", "record/call/7/name"));
        assert!(!matches_path(
            "record/call/INDEX/name",
            "record/call/no/name"
        ));
        let choices = choices("network/prefix", None).unwrap();
        assert_eq!(choices["candidates"].as_array().unwrap().len(), 256);
        assert_eq!(choices["candidates"][255]["label"], "Code 255");
        let packed = super::choices("choice/A2-H56095-01/0", None).unwrap();
        assert_eq!(packed["candidates"].as_array().unwrap().len(), 4);
        assert!(describe("unknown-field").is_err());
    }

    #[test]
    fn usb_choices_use_live_values_without_replacing_unknown_codes() {
        let mut raw = vec![0; 0x3c9c0];
        raw[0x355a1] = 6;
        raw[0x3c9b0..].copy_from_slice(b"IcomCloneFormat3");
        let mut image = Icf::from_memory(raw).unwrap();
        let send = settings::catalog().iter().find(|s| s.key == 0x2fb).unwrap();
        let keying = settings::catalog().iter().find(|s| s.key == 0x2fc).unwrap();
        for (setting, candidate) in [(send, 1), (keying, 255)] {
            setting
                .select(
                    &mut image,
                    0,
                    settings::Selection {
                        key: setting.selection_key,
                        phase: 2,
                        candidate,
                        accepted: true,
                    },
                )
                .unwrap();
        }
        let before = image.memory().to_vec();
        let path = format!("setting/{}/0", keying.name);
        assert_eq!(keying.feature_id, "A2-H56399-16");
        assert!(choices(&path, None).is_err());
        assert!(choices(keying.feature_id, None).is_err());
        let by_path = choices(&path, Some(&image)).unwrap();
        let by_id = choices(keying.feature_id, Some(&image)).unwrap();
        assert_eq!(by_id, by_path);
        assert_eq!(by_path["current"], 255);
        assert!(
            !by_path["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["value"] == 1)
        );
        assert_eq!(image.memory(), before);
    }

    #[test]
    fn explicit_choice_path_keeps_its_requested_index() {
        let editable = choices("choice/agc/0", None).unwrap();
        let fixed = choices("choice/agc/4", None).unwrap();
        assert_eq!(editable["field"]["key"], fixed["field"]["key"]);
        assert_eq!(editable["candidates"].as_array().unwrap().len(), 3);
        assert_eq!(fixed["candidates"], json!([]));
    }
}
