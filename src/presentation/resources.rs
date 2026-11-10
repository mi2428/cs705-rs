//! Compiled, independent language resources with checked named arguments.
//! TSV values are UTF-8, single-line text. Only `{name}` placeholders are used;
//! this is not a general template language or a source-data character codec.

use super::Language;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

const SOURCES: [&str; 6] = [
    include_str!("locales/en.tsv"),
    include_str!("locales/ja.tsv"),
    include_str!("locales/de.tsv"),
    include_str!("locales/fr.tsv"),
    include_str!("locales/es.tsv"),
    include_str!("locales/it.tsv"),
];

type Catalog = BTreeMap<&'static str, &'static str>;

fn placeholders(text: &str) -> Result<BTreeSet<&str>, &'static str> {
    let mut names = BTreeSet::new();
    let mut rest = text;
    while let Some((before, after)) = rest.split_once('{') {
        if before.contains('}') {
            return Err("unmatched resource brace");
        }
        let (name, tail) = after
            .split_once('}')
            .ok_or("unclosed resource placeholder")?;
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
            return Err("invalid resource placeholder");
        }
        names.insert(name);
        rest = tail;
    }
    if rest.contains('}') {
        return Err("unmatched resource brace");
    }
    Ok(names)
}

fn parse(source: &'static str) -> Result<Catalog, &'static str> {
    let mut catalog = BTreeMap::new();
    for line in source
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let (id, text) = line.split_once('\t').ok_or("missing resource separator")?;
        if id.is_empty()
            || !id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_'))
        {
            return Err("invalid resource ID");
        }
        if text.is_empty() || text.contains('\t') || text.chars().any(char::is_control) {
            return Err("invalid resource text");
        }
        placeholders(text)?;
        if catalog.insert(id, text).is_some() {
            return Err("duplicate resource ID");
        }
    }
    if catalog.is_empty() {
        return Err("empty resource catalog");
    }
    Ok(catalog)
}

fn catalogs() -> &'static [Catalog; 6] {
    static CATALOGS: OnceLock<[Catalog; 6]> = OnceLock::new();
    CATALOGS.get_or_init(|| {
        let catalogs =
            SOURCES.map(|source| parse(source).expect("valid compiled language resource"));
        let base = &catalogs[Language::English as usize];
        for catalog in &catalogs {
            assert!(
                base.keys().eq(catalog.keys()),
                "complete compiled language resource IDs"
            );
            for (id, text) in catalog {
                assert_eq!(
                    placeholders(base[id]),
                    placeholders(text),
                    "compiled resource arguments: {id}"
                );
                if id.starts_with("field.")
                    || id.starts_with("choice.")
                    || *id == "repeater.gateway_reference"
                {
                    assert!(
                        placeholders(text)
                            .expect("valid resource placeholders")
                            .is_empty(),
                        "static field label has arguments: {id}"
                    );
                }
            }
        }
        catalogs
    })
}

pub(super) fn lookup(language: Language, id: &str) -> Option<&'static str> {
    catalogs()[language as usize].get(id).copied()
}

fn render(template: &str, args: &[(&str, &str)]) -> Result<String, &'static str> {
    let expected = placeholders(template)?;
    let supplied: BTreeSet<_> = args.iter().map(|(name, _)| *name).collect();
    if supplied.len() != args.len() || supplied != expected {
        return Err("resource argument mismatch");
    }
    let mut text = String::new();
    let mut rest = template;
    while let Some((before, after)) = rest.split_once('{') {
        let (name, tail) = after
            .split_once('}')
            .ok_or("unclosed resource placeholder")?;
        text.push_str(before);
        text.push_str(
            args.iter()
                .find(|(key, _)| *key == name)
                .ok_or("missing resource argument")?
                .1,
        );
        rest = tail;
    }
    text.push_str(rest);
    Ok(text)
}

pub(super) fn format(language: Language, id: &str, args: &[(&str, &str)]) -> String {
    let template = lookup(language, id).expect("registered typed resource ID");
    render(template, args).expect("typed resource arguments")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_identical_ids_and_named_arguments() {
        let catalogs = catalogs();
        assert_eq!(catalogs.len(), Language::ALL.len());
        assert!(catalogs.iter().all(|catalog| !catalog.is_empty()));
        for language in Language::ALL {
            assert!(lookup(language, "missing.message").is_none());
            for (id, template) in &catalogs[language as usize] {
                let names = placeholders(template).unwrap();
                let args: Vec<_> = names.into_iter().map(|name| (name, "17")).collect();
                assert!(!format(language, id, &args).is_empty(), "{id}");
            }
        }
    }

    #[test]
    fn malformed_catalogs_and_templates_fail_closed() {
        for source in [
            "",
            "a\tx\na\ty",
            "a\t",
            "a x\ty",
            "a\tx\ty",
            "a\t{x",
            "a\tx}",
            "a\t{}",
            "a\t{x{y}",
        ] {
            assert!(parse(source).is_err(), "{source:?}");
        }
        assert_eq!(
            render(
                "{second}/{first}/{second}",
                &[("first", "A"), ("second", "B")]
            ),
            Ok("B/A/B".into())
        );
        assert!(render("{value}", &[]).is_err());
        assert!(render("literal", &[("value", "1")]).is_err());
        assert!(render("{value}", &[("value", "1"), ("value", "2")]).is_err());
        assert_eq!(
            render("{value}", &[("value", "{not_a_template}")]),
            Ok("{not_a_template}".into())
        );
    }
}
