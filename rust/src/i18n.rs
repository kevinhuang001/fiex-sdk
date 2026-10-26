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
