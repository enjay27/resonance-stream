//! Furigana for a Japanese chat line: the line cut into [`RubySpan`]s, each
//! piece with kanji carrying its hiragana reading.
//!
//! [`spans_from_tokens`] is the pure part: it takes a morphological analyser's
//! tokens (surface text and katakana reading) and puts the reading over the
//! kanji only, leaving okurigana (走**った**) plain. Which analyser makes the
//! tokens is the adapter's business, so this part is tested without one.

use lindera::dictionary::load_dictionary;
use lindera::mode::Mode;
use lindera::segmenter::Segmenter;
use resonance_types::{contains_japanese, RubySpan};
use std::borrow::Cow;
use std::sync::OnceLock;

/// One analysed word: the text as written, and its reading in katakana when
/// the analyser knows it.
pub type Token<'a> = (&'a str, Option<&'a str>);

/// The line as [`RubySpan`]s. The spans' text, joined, is exactly the tokens'
/// surfaces, joined; neighbouring plain pieces are merged into one span.
pub fn spans_from_tokens<'a>(tokens: impl IntoIterator<Item = Token<'a>>) -> Vec<RubySpan> {
    let mut out = Vec::new();
    for (surface, reading) in tokens {
        let reading = reading
            .filter(|r| *r != "*")
            .map(to_hiragana)
            .filter(|r| !r.is_empty() && r.chars().all(is_hiragana_or_mark));
        match reading {
            Some(reading) if surface.chars().any(is_kanji) => {
                for span in align(surface, &reading) {
                    push(&mut out, span);
                }
            }
            _ => push(&mut out, RubySpan::plain(surface)),
        }
    }
    out
}

/// A word's surface cut so the reading sits over its kanji only. When the
/// word's kana and its reading do not line up, the whole word carries the
/// reading: a coarser ruby, never a wrong one.
fn align(surface: &str, reading: &str) -> Vec<RubySpan> {
    let runs = runs(surface);
    let reading_chars: Vec<char> = reading.chars().collect();
    split(&runs, &reading_chars).unwrap_or_else(|| vec![RubySpan::with_reading(surface, reading)])
}

/// A stretch of a word: all kanji, or none of it.
struct Run<'a> {
    kanji: bool,
    text: &'a str,
}

fn runs(surface: &str) -> Vec<Run<'_>> {
    let mut runs: Vec<Run> = Vec::new();
    let mut start = 0;
    let mut kanji = None;
    for (i, c) in surface.char_indices() {
        let is = is_kanji(c);
        if kanji.is_some_and(|k| k != is) {
            runs.push(Run {
                kanji: !is,
                text: &surface[start..i],
            });
            start = i;
        }
        kanji = Some(is);
    }
    if let Some(kanji) = kanji {
        runs.push(Run {
            kanji,
            text: &surface[start..],
        });
    }
    runs
}

/// Lays `reading` over `runs`: a kana run must be the next kana of the
/// reading, a kanji run takes what lies before the kana run that follows it
/// (at least one kana), or the rest. `None` when it cannot be done.
fn split(runs: &[Run], reading: &[char]) -> Option<Vec<RubySpan>> {
    let Some((run, rest)) = runs.split_first() else {
        return reading.is_empty().then(Vec::new);
    };
    if !run.kanji {
        let kana: Vec<char> = run.text.chars().map(hiragana).collect();
        let tail = reading.strip_prefix(kana.as_slice())?;
        let mut spans = vec![RubySpan::plain(run.text)];
        spans.extend(split(rest, tail)?);
        return Some(spans);
    }
    if rest.is_empty() {
        return (!reading.is_empty()).then(|| {
            vec![RubySpan::with_reading(
                run.text,
                reading.iter().collect::<String>(),
            )]
        });
    }
    // A kanji run is followed by a kana run (runs alternate): find where that
    // kana starts in the reading, leftmost first, and back off if the rest
    // does not fit.
    let kana: Vec<char> = rest[0].text.chars().map(hiragana).collect();
    (1..reading.len()).find_map(|at| {
        reading[at..].starts_with(&kana).then(|| {
            let mut spans = vec![RubySpan::with_reading(
                run.text,
                reading[..at].iter().collect::<String>(),
            )];
            spans.extend(split(rest, &reading[at..])?);
            Some(spans)
        })?
    })
}

/// Adds `span`, joining it to the span before it when both are plain.
fn push(spans: &mut Vec<RubySpan>, span: RubySpan) {
    match spans.last_mut() {
        Some(last) if last.reading.is_none() && span.reading.is_none() => {
            last.text.push_str(&span.text)
        }
        _ => spans.push(span),
    }
}

fn is_kanji(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '々')
}

fn is_hiragana_or_mark(c: char) -> bool {
    matches!(c, '\u{3041}'..='\u{3096}' | 'ー')
}

/// Katakana as hiragana; every other character as it is.
fn hiragana(c: char) -> char {
    match c {
        '\u{30A1}'..='\u{30F6}' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
        _ => c,
    }
}

fn to_hiragana(text: &str) -> String {
    text.chars().map(hiragana).collect()
}

/// The analyser: lindera with the IPADIC dictionary embedded in the binary.
pub struct Furigana {
    segmenter: Segmenter,
}

impl Furigana {
    pub fn new() -> Result<Self, String> {
        let dictionary = load_dictionary("embedded://ipadic").map_err(|e| e.to_string())?;
        Ok(Self {
            segmenter: Segmenter::new(Mode::Normal, dictionary, None),
        })
    }

    /// `text` as [`RubySpan`]s. A line that cannot be analysed (or has no
    /// Japanese in it) comes back as one plain span.
    pub fn annotate(&self, text: &str) -> Vec<RubySpan> {
        if text.is_empty() {
            return Vec::new();
        }
        if !contains_japanese(text) {
            return vec![RubySpan::plain(text)];
        }
        let mut tokens = match self.segmenter.segment(Cow::Borrowed(text)) {
            Ok(tokens) => tokens,
            Err(e) => {
                log::warn!("furigana: could not analyse a line: {e}");
                return vec![RubySpan::plain(text)];
            }
        };
        // IPADIC details: ..., [7] reading (katakana), [8] pronunciation.
        let words: Vec<(String, Option<String>)> = tokens
            .iter_mut()
            .map(|token| {
                let surface = token.surface.to_string();
                let reading = token.details().get(7).map(|r| r.to_string());
                (surface, reading)
            })
            .collect();
        // The analyser must not lose or change a character (it may drop
        // spaces): a line that does not read back as itself is shown plain.
        if words.iter().map(|(s, _)| s.as_str()).collect::<String>() != text {
            return vec![RubySpan::plain(text)];
        }
        spans_from_tokens(words.iter().map(|(s, r)| (s.as_str(), r.as_deref())))
    }
}

/// [`Furigana::annotate`] with one analyser for the whole process, loaded on
/// first use. Without a working dictionary every line is plain.
pub fn annotate(text: &str) -> Vec<RubySpan> {
    static ANALYSER: OnceLock<Option<Furigana>> = OnceLock::new();
    let analyser = ANALYSER.get_or_init(|| match Furigana::new() {
        Ok(analyser) => Some(analyser),
        Err(e) => {
            log::error!("furigana dictionary did not load: {e}");
            None
        }
    });
    match analyser {
        Some(analyser) => analyser.annotate(text),
        None if text.is_empty() => Vec::new(),
        None => vec![RubySpan::plain(text)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(tokens: &[Token]) -> Vec<RubySpan> {
        spans_from_tokens(tokens.iter().copied())
    }

    fn r(text: &str, reading: &str) -> RubySpan {
        RubySpan::with_reading(text, reading)
    }

    fn p(text: &str) -> RubySpan {
        RubySpan::plain(text)
    }

    #[test]
    fn each_word_with_kanji_gets_its_reading_in_hiragana() {
        assert_eq!(
            spans(&[
                ("日", Some("ニチ")),
                ("韓", Some("カン")),
                ("辞書", Some("ジショ"))
            ]),
            [r("日", "にち"), r("韓", "かん"), r("辞書", "じしょ")]
        );
    }

    #[test]
    fn okurigana_stays_plain_and_neighbouring_plain_pieces_merge() {
        assert_eq!(
            spans(&[("走っ", Some("ハシッ")), ("た", Some("タ"))]),
            [r("走", "はし"), p("った")]
        );
    }

    #[test]
    fn a_word_with_kana_between_its_kanji_splits_on_them() {
        assert_eq!(
            spans(&[("取り込む", Some("トリコム"))]),
            [r("取", "と"), p("り"), r("込", "こ"), p("む")]
        );
        assert_eq!(
            spans(&[("思い出す", Some("オモイダス"))]),
            [r("思", "おも"), p("い"), r("出", "だ"), p("す")]
        );
    }

    #[test]
    fn a_leading_kana_is_left_plain() {
        assert_eq!(
            spans(&[("お疲れ様", Some("オツカレサマ"))]),
            [p("お"), r("疲", "つか"), p("れ"), r("様", "さま")]
        );
    }

    #[test]
    fn words_without_kanji_or_without_a_reading_are_plain_and_merge() {
        assert_eq!(
            spans(&[
                ("パーティー", Some("パーティー")),
                ("を", Some("ヲ")),
                ("22", None),
                ("、", None),
            ]),
            [p("パーティーを22、")]
        );
        // The analyser did not know the word: no guess.
        assert_eq!(spans(&[("深淵", None)]), [p("深淵")]);
        assert_eq!(spans(&[("深淵", Some("*"))]), [p("深淵")]);
        assert_eq!(spans(&[("深淵", Some(""))]), [p("深淵")]);
    }

    #[test]
    fn the_iteration_mark_belongs_to_the_kanji_before_it() {
        assert_eq!(
            spans(&[("人々", Some("ヒトビト"))]),
            [r("人々", "ひとびと")]
        );
    }

    #[test]
    fn a_word_that_does_not_line_up_gets_one_reading_over_all_of_it() {
        // ヶ is read か here: the kana of the word and its reading disagree.
        assert_eq!(
            spans(&[("一ヶ月", Some("イッカゲツ"))]),
            [r("一ヶ月", "いっかげつ")]
        );
        // Anchor kana the reading does not contain.
        assert_eq!(spans(&[("食べる", Some("ノム"))]), [r("食べる", "のむ")]);
    }

    #[test]
    fn the_spans_always_read_back_as_the_line_and_only_kanji_carry_readings() {
        let tokens: &[Token] = &[
            ("お疲れ様", Some("オツカレサマ")),
            ("です", Some("デス")),
            ("、", None),
            ("取り込む", Some("トリコム")),
            ("一ヶ月", Some("イッカゲツ")),
            ("22", None),
            ("人々", Some("ヒトビト")),
            ("深淵", None),
            ("ありがとう", Some("アリガトウ")),
        ];
        let line: String = tokens.iter().map(|(surface, _)| *surface).collect();
        let got = spans(tokens);
        assert_eq!(
            got.iter().map(|s| s.text.as_str()).collect::<String>(),
            line
        );
        for span in &got {
            assert!(!span.text.is_empty());
            if let Some(reading) = &span.reading {
                assert!(span.text.chars().any(is_kanji), "{span:?}");
                assert!(!reading.is_empty());
                assert!(reading.chars().all(is_hiragana_or_mark), "{span:?}");
            }
        }
    }

    #[test]
    fn no_tokens_is_no_spans() {
        assert_eq!(spans(&[]), []);
    }

    fn is_kanji(c: char) -> bool {
        matches!(c, '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '々')
    }

    fn is_hiragana_or_mark(c: char) -> bool {
        matches!(c, '\u{3041}'..='\u{3096}' | 'ー')
    }

    // The dictionary-backed tests below load the real IPADIC.

    fn analyser() -> Furigana {
        Furigana::new().expect("the embedded dictionary loads")
    }

    #[test]
    fn a_line_is_read_word_by_word_from_the_dictionary() {
        let f = analyser();
        assert_eq!(
            f.annotate("日韓辞書"),
            [r("日", "にち"), r("韓", "かん"), r("辞書", "じしょ")]
        );
        assert_eq!(
            f.annotate("今日はパーティー募集します"),
            [
                r("今日", "きょう"),
                p("はパーティー"),
                r("募集", "ぼしゅう"),
                p("します")
            ]
        );
        assert_eq!(
            f.annotate("明日22時から"),
            [r("明日", "あした"), p("22"), r("時", "じ"), p("から")]
        );
    }

    #[test]
    fn a_line_without_kanji_is_one_plain_span_and_nothing_is_nothing() {
        let f = analyser();
        assert_eq!(f.annotate("ありがとう!"), [p("ありがとう!")]);
        assert_eq!(f.annotate("[스티커]よろしく"), [p("[스티커]よろしく")]);
        assert_eq!(f.annotate("hello"), [p("hello")]);
        assert_eq!(f.annotate(""), []);
    }

    #[test]
    fn a_real_line_reads_back_as_itself() {
        let f = analyser();
        for line in [
            "お疲れ様です、周回手伝ってください",
            "明日22時から深淵行く人いますか",
            "@abc 募集 [스티커] 大人気の剣士",
            "ｱｲﾃﾑ　全角スペースと半角 space",
        ] {
            let got = f.annotate(line);
            assert_eq!(
                got.iter().map(|s| s.text.as_str()).collect::<String>(),
                line
            );
            assert!(got.iter().all(|s| !s.text.is_empty()), "{got:?}");
        }
    }

    #[test]
    fn the_shared_analyser_answers_like_a_fresh_one() {
        assert_eq!(annotate("日韓辞書"), analyser().annotate("日韓辞書"));
    }
}
