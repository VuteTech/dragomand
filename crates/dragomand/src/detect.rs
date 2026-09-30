// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Language identification for `DetectLanguage`: trigram statistics
//! (whatlang), fully offline, no model files, microseconds per call.

use whatlang::{Detector, Lang};

/// The outcome of a detection.
#[derive(Debug, Clone, PartialEq)]
pub struct Detection {
    /// A language code as the providers use it ("bg", "zh-Hant"); `None`
    /// when the text gives nothing to go on.
    pub language: Option<String>,
    /// 0 to 1.
    pub confidence: f64,
    /// Whether the detector trusts its answer (enough text, a clear lead).
    pub reliable: bool,
}

/// The provider's language code for a whatlang language.
fn code(lang: Lang) -> &'static str {
    match lang {
        Lang::Afr => "af",
        Lang::Aka => "ak",
        Lang::Amh => "am",
        Lang::Ara => "ar",
        Lang::Aze => "az",
        Lang::Bel => "be",
        Lang::Ben => "bn",
        Lang::Bul => "bg",
        Lang::Cat => "ca",
        Lang::Ces => "cs",
        Lang::Cmn => "zh",
        Lang::Cym => "cy",
        Lang::Dan => "da",
        Lang::Deu => "de",
        Lang::Ell => "el",
        Lang::Eng => "en",
        Lang::Epo => "eo",
        Lang::Est => "et",
        Lang::Fin => "fi",
        Lang::Fra => "fr",
        Lang::Guj => "gu",
        Lang::Heb => "he",
        Lang::Hin => "hi",
        Lang::Hrv => "hr",
        Lang::Hun => "hu",
        Lang::Hye => "hy",
        Lang::Ind => "id",
        Lang::Ita => "it",
        Lang::Jav => "jv",
        Lang::Jpn => "ja",
        Lang::Kan => "kn",
        Lang::Kat => "ka",
        Lang::Khm => "km",
        Lang::Kor => "ko",
        Lang::Lat => "la",
        Lang::Lav => "lv",
        Lang::Lit => "lt",
        Lang::Mal => "ml",
        Lang::Mar => "mr",
        Lang::Mkd => "mk",
        Lang::Mya => "my",
        Lang::Nep => "ne",
        Lang::Nld => "nl",
        Lang::Nob => "nb",
        Lang::Ori => "or",
        Lang::Pan => "pa",
        Lang::Pes => "fa",
        Lang::Pol => "pl",
        Lang::Por => "pt",
        Lang::Ron => "ro",
        Lang::Rus => "ru",
        Lang::Sin => "si",
        Lang::Slk => "sk",
        Lang::Slv => "sl",
        Lang::Sna => "sn",
        Lang::Spa => "es",
        Lang::Srp => "sr",
        Lang::Swe => "sv",
        Lang::Tam => "ta",
        Lang::Tel => "te",
        Lang::Tgl => "tl",
        Lang::Tha => "th",
        Lang::Tuk => "tk",
        Lang::Tur => "tr",
        Lang::Ukr => "uk",
        Lang::Urd => "ur",
        Lang::Uzb => "uz",
        Lang::Vie => "vi",
        Lang::Yid => "yi",
        Lang::Zul => "zu",
    }
}

/// The whatlang language for a provider code; the script subtag of
/// Chinese is resolved after detection.
fn lang(code: &str) -> Option<Lang> {
    let primary = code.split(['-', '_']).next().unwrap_or(code);
    Lang::all()
        .iter()
        .copied()
        .find(|&l| self::code(l) == primary)
}

/// Simplified or Traditional Chinese, by characters only one script uses.
fn chinese_variant(text: &str) -> &'static str {
    const SIMPLIFIED: &str =
        "们这个来时说国为会学对发经么还过后里点开见问长间无与关当实现头样话电车门东语";
    const TRADITIONAL: &str =
        "們這個來時說國為會學對發經麼還過後裡點開見問長間無與關當實現頭樣話電車門東語";
    let count = |set: &str| text.chars().filter(|c| set.contains(*c)).count();
    if count(TRADITIONAL) > count(SIMPLIFIED) {
        "zh-Hant"
    } else {
        "zh-Hans"
    }
}

/// Detects the language of `text`, restricted to `candidates` (provider
/// codes) when any are given. Candidates the detector does not know are
/// ignored; when none of them is known, nothing is detected.
pub fn detect(text: &str, candidates: &[String]) -> Detection {
    let nothing = Detection {
        language: None,
        confidence: 0.0,
        reliable: false,
    };
    let detector = if candidates.is_empty() {
        Detector::new()
    } else {
        let mut allowed: Vec<Lang> = candidates.iter().filter_map(|c| lang(c)).collect();
        allowed.sort_by_key(|l| l.code());
        allowed.dedup();
        if allowed.is_empty() {
            return nothing;
        }
        Detector::with_allowlist(allowed)
    };
    let Some(info) = detector.detect(text) else {
        return nothing;
    };
    let language = match info.lang() {
        Lang::Cmn => chinese_variant(text),
        other => code(other),
    };
    Detection {
        language: Some(language.to_owned()),
        confidence: info.confidence().clamp(0.0, 1.0),
        reliable: info.is_reliable(),
    }
}

#[cfg(test)]
mod tests {
    use super::detect;

    /// The languages Mozilla publishes models for (September 2026).
    fn mozilla_languages() -> Vec<String> {
        "af ar az bg bn bs ca cs da de el en es et eu fa fi fr gl gu he hi hr hu id is it ja kn ko lt lv ml mr ms nb nl pl pt ro ru sk sl sq sr sv ta te th tr uk ur vi zh-Hans zh-Hant"
            .split(' ')
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn detects_common_languages() {
        let known = mozilla_languages();
        for text in [
            "Добро утро, как си днес? Времето е прекрасно.",
            "Котката спи на дивана в хола.",
            "Утре ще ходим на планина с приятели.",
        ] {
            let bulgarian = detect(text, &known);
            assert_eq!(bulgarian.language.as_deref(), Some("bg"), "{text}");
        }
        let english = detect(
            "Good morning, how are you today? The weather is lovely.",
            &known,
        );
        assert_eq!(english.language.as_deref(), Some("en"));
        assert!(english.reliable);
    }

    #[test]
    fn candidates_restrict_the_answer() {
        let text = "Добро утро, как си днес?";
        let restricted = detect(text, &["ru".into(), "uk".into()]);
        assert!(matches!(restricted.language.as_deref(), Some("ru" | "uk")));
        assert_eq!(detect(text, &["xx".into()]).language, None);
    }

    #[test]
    fn tells_chinese_scripts_apart() {
        assert_eq!(
            detect("我们这个时候说话", &[]).language.as_deref(),
            Some("zh-Hans")
        );
        assert_eq!(
            detect("我們這個時候說話", &[]).language.as_deref(),
            Some("zh-Hant")
        );
        assert_eq!(
            detect("我們這個時候說話", &["zh-Hant".into(), "ja".into()])
                .language
                .as_deref(),
            Some("zh-Hant")
        );
    }

    #[test]
    fn no_letters_no_language() {
        assert_eq!(detect("12345 !!!", &[]).language, None);
    }
}
