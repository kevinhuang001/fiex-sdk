//! Request-local language inherited by background tasks. No process-wide locale.
use std::cell::Cell;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Locale {
    #[default]
    SimplifiedChinese,
    TraditionalChinese,
    English,
}
impl Locale {
    pub fn parse(value: &str) -> Self {
        let tag = value.replace('_', "-").to_ascii_lowercase();
        if tag == "en" || tag.starts_with("en-") {
            Self::English
        } else if ["zh-tw", "zh-hk", "zh-mo", "zh-hant"]
            .iter()
            .any(|p| tag == *p || tag.starts_with(&format!("{p}-")))
        {
            Self::TraditionalChinese
        } else {
            Self::SimplifiedChinese
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SimplifiedChinese => "zh-CN",
            Self::TraditionalChinese => "zh-TW",
            Self::English => "en",
        }
    }
}
thread_local! { static CURRENT: Cell<Locale> = const { Cell::new(Locale::SimplifiedChinese) }; }
pub fn current_locale() -> Locale {
    CURRENT.with(Cell::get)
}
/// Run synchronous request work in its captured language and restore on unwind.
pub fn with_locale<T>(locale: Locale, work: impl FnOnce() -> T) -> T {
    struct Restore(Locale);
    impl Drop for Restore {
        fn drop(&mut self) {
            CURRENT.with(|value| value.set(self.0));
        }
    }
    let _restore = Restore(CURRENT.with(|value| value.replace(locale)));
    work()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalize_and_restore_locale() {
        assert_eq!(Locale::parse("zh-Hant-HK"), Locale::TraditionalChinese);
        assert_eq!(Locale::parse("en-US"), Locale::English);
        with_locale(Locale::English, || {
            assert_eq!(current_locale(), Locale::English)
        });
        assert_eq!(current_locale(), Locale::SimplifiedChinese);
    }
    #[test]
    fn concurrent_languages_do_not_leak() {
        let tasks = [Locale::English, Locale::TraditionalChinese].map(|locale| {
            std::thread::spawn(move || {
                with_locale(locale, || {
                    std::thread::yield_now();
                    assert_eq!(current_locale(), locale);
                });
            })
        });
        for task in tasks {
            task.join().unwrap();
        }
    }
}

/// Source-key catalogs shared by native plugins. Missing entries fall back to source.
#[derive(Default)]
pub struct Catalog {
    english: std::collections::BTreeMap<String, String>,
    traditional: std::collections::BTreeMap<String, String>,
}
impl Catalog {
    pub fn from_json(english: &str, traditional: &str) -> Result<Self, serde_json::Error> {
        Ok(Self {
            english: serde_json::from_str(english)?,
            traditional: serde_json::from_str(traditional)?,
        })
    }

    pub fn t<'a>(&'a self, source: &'a str) -> &'a str {
        let catalog = match current_locale() {
            Locale::English => &self.english,
            Locale::TraditionalChinese => &self.traditional,
            Locale::SimplifiedChinese => return source,
        };
        catalog.get(source).map(String::as_str).unwrap_or(source)
    }
}

/// Substitute named or numbered slots once, preserving braces in user data.
pub fn format(template: &str, values: &[(&str, String)]) -> String {
    let mut rest = template;
    let mut output = String::new();
    while let Some(start) = rest.find('{') {
        output.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}').map(|end| start + end) else {
            output.push_str(&rest[start..]);
            return output;
        };
        let name = &rest[start + 1..end];
        if let Some((_, value)) = values.iter().find(|(key, _)| *key == name) {
            output.push_str(value);
        } else {
            output.push_str(&rest[start..=end]);
        }
        rest = &rest[end + 1..];
    }
    output.push_str(rest);
    output
}

#[cfg(test)]
mod catalog_tests {
    use super::*;

    #[test]
    fn source_fallback_and_request_scoped_catalogs() {
        let catalog =
            Catalog::from_json(r#"{"打开 {0}":"Open {0}"}"#, r#"{"打开 {0}":"開啟 {0}"}"#).unwrap();
        for (locale, expected) in [
            (Locale::SimplifiedChinese, "打开 {0}"),
            (Locale::TraditionalChinese, "開啟 {0}"),
            (Locale::English, "Open {0}"),
        ] {
            with_locale(locale, || {
                assert_eq!(catalog.t("打开 {0}"), expected);
                assert_eq!(catalog.t("missing"), "missing");
            });
        }
    }

    #[test]
    fn interpolation_does_not_reinterpret_user_values() {
        assert_eq!(
            format(
                "To {1} from {0}",
                &[("0", "中文{1}.txt".into()), ("1", "B".into())]
            ),
            "To B from 中文{1}.txt"
        );
        assert_eq!(format("{unknown} {", &[]), "{unknown} {");
    }
}
