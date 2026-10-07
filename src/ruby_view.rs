//! Furigana in a chat row: the spans the backend returned, with the user's
//! emphasis keywords marked over them, and a cache of what was already asked.

use crate::ui_types::RubySpan;
use std::collections::{HashMap, VecDeque};

/// One thing to draw: a stretch of the line, its reading when it has one, and
/// whether an emphasis keyword covers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub text: String,
    pub reading: Option<String>,
    pub emphasized: bool,
}

/// `spans` with the places the `keywords` occur in the line marked, found the
/// way `chat_row::render_emphasized` finds them. A plain span is cut at a
/// keyword's edges; a span with a reading is one unit, marked as a whole when
/// a keyword touches it (cutting a reading in two would misplace it).
pub fn mark(spans: &[RubySpan], keywords: &[String]) -> Vec<Piece> {
    let text: String = spans.iter().map(|s| s.text.as_str()).collect();
    let ranges = keyword_ranges(&text, keywords);
    let piece = |text: &str, reading: Option<&String>, emphasized: bool| Piece {
        text: text.to_string(),
        reading: reading.cloned(),
        emphasized,
    };
    let mut pieces = Vec::new();
    let mut start = 0;
    for span in spans {
        let end = start + span.text.len();
        if span.reading.is_some() {
            let touched = ranges.iter().any(|&(a, b)| a < end && b > start);
            pieces.push(piece(&span.text, span.reading.as_ref(), touched));
        } else {
            let mut at = start;
            for &(a, b) in &ranges {
                if b <= start || a >= end {
                    continue;
                }
                let (a, b) = (a.max(start), b.min(end));
                if a > at {
                    pieces.push(piece(&text[at..a], None, false));
                }
                pieces.push(piece(&text[a..b], None, true));
                at = b;
            }
            if at < end {
                pieces.push(piece(&text[at..end], None, false));
            }
        }
        start = end;
    }
    pieces
}

/// Byte ranges of the keywords in `text`, left to right: at each step the
/// keyword that starts earliest (the one listed first on a tie), then on
/// after it -- the search `chat_row::render_emphasized` makes.
fn keyword_ranges(text: &str, keywords: &[String]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut from = 0;
    while from < text.len() {
        let rest = &text[from..];
        let mut earliest: Option<(usize, usize)> = None;
        for keyword in keywords.iter().filter(|k| !k.is_empty()) {
            if let Some(at) = rest.find(keyword.as_str()) {
                if earliest.map_or(true, |(best, _)| at < best) {
                    earliest = Some((at, keyword.len()));
                }
            }
        }
        let Some((at, len)) = earliest else { break };
        ranges.push((from + at, from + at + len));
        from += at + len;
    }
    ranges
}

/// What the backend answered per line, newest kept: a row that is built again
/// (tab switch, paging) does not ask again.
pub struct RubyCache {
    capacity: usize,
    lines: HashMap<String, Vec<RubySpan>>,
    order: VecDeque<String>,
}

impl RubyCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            lines: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn get(&self, line: &str) -> Option<&Vec<RubySpan>> {
        self.lines.get(line)
    }

    pub fn put(&mut self, line: &str, spans: Vec<RubySpan>) {
        if self.capacity == 0 {
            return;
        }
        if self.lines.insert(line.to_string(), spans).is_some() {
            return; // already counted
        }
        self.order.push_back(line.to_string());
        while self.lines.len() > self.capacity {
            match self.order.pop_front() {
                Some(oldest) => self.lines.remove(&oldest),
                None => break,
            };
        }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// Most distinct lines one `annotate_furigana` call carries.
pub const RUBY_BATCH_MAX: usize = 100;

/// Rows that ask for furigana close together (a tab switch builds them all at once) are answered by
/// one backend call instead of one call each (P-3). A row adds its line and something to call when the
/// answer comes (`C`); a line asked by several rows is asked once. The caller starts a short timer
/// when `add` says the window opened and drains the batch when it fires.
pub struct RubyBatch<C> {
    lines: Vec<String>,
    waiting: HashMap<String, Vec<C>>,
}

impl<C> RubyBatch<C> {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            waiting: HashMap::new(),
        }
    }

    /// Adds a row's request. `true` when the batch was empty: this request opens the window.
    pub fn add(&mut self, line: &str, row: C) -> bool {
        let opens = self.lines.is_empty();
        match self.waiting.get_mut(line) {
            Some(rows) => rows.push(row),
            None => {
                self.lines.push(line.to_string());
                self.waiting.insert(line.to_string(), vec![row]);
            }
        }
        opens
    }

    /// Takes everything asked so far as calls of at most `max` distinct lines each, in the order the
    /// lines were first asked.
    pub fn drain(&mut self, max: usize) -> Vec<RubyCall<C>> {
        let lines = std::mem::take(&mut self.lines);
        let mut waiting = std::mem::take(&mut self.waiting);
        lines
            .chunks(max.max(1))
            .map(|chunk| RubyCall {
                lines: chunk.to_vec(),
                waiting: chunk
                    .iter()
                    .map(|line| waiting.remove(line).unwrap_or_default())
                    .collect(),
            })
            .collect()
    }
}

impl<C> Default for RubyBatch<C> {
    fn default() -> Self {
        Self::new()
    }
}

/// One backend call: its lines, and the rows waiting for each.
pub struct RubyCall<C> {
    lines: Vec<String>,
    waiting: Vec<Vec<C>>,
}

impl<C> RubyCall<C> {
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Pairs the backend's answer (one list of spans per line, in order) with the lines and the rows
    /// waiting for each. A line the answer does not reach stays unanswered, and an answer longer than
    /// the question is cut: the row simply shows no furigana, as after a failed call.
    pub fn answer(self, spans: Vec<Vec<RubySpan>>) -> Vec<(String, Vec<RubySpan>, Vec<C>)> {
        self.lines
            .into_iter()
            .zip(spans)
            .zip(self.waiting)
            .map(|((line, spans), rows)| (line, spans, rows))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_asking_in_one_window_become_one_call_with_each_line_once() {
        let mut batch = RubyBatch::new();
        assert!(batch.add("あ", 1), "the first request starts the window");
        assert!(!batch.add("い", 2));
        assert!(!batch.add("あ", 3), "the same line from another row");
        let calls = batch.drain(100);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].lines(), ["あ", "い"], "in the order first asked");
        assert!(batch.drain(100).is_empty(), "drained once");
        assert!(batch.add("う", 4), "an empty batch starts a new window");
    }

    #[test]
    fn a_long_batch_is_cut_into_calls_of_at_most_max_lines() {
        let mut batch = RubyBatch::new();
        for i in 0..5 {
            batch.add(&format!("line{i}"), i);
        }
        let calls = batch.drain(2);
        let sizes: Vec<usize> = calls.iter().map(|c| c.lines().len()).collect();
        assert_eq!(sizes, [2, 2, 1]);
        assert_eq!(calls[2].lines(), ["line4"]);
    }

    #[test]
    fn each_waiting_row_gets_the_spans_of_its_own_line() {
        let mut batch = RubyBatch::new();
        batch.add("あ", 1);
        batch.add("い", 2);
        batch.add("あ", 3);
        let call = batch.drain(100).pop().unwrap();
        let answered = call.answer(vec![vec![p("あ")], vec![r("い", "i")]]);
        assert_eq!(
            answered,
            [
                ("あ".to_string(), vec![p("あ")], vec![1, 3]),
                ("い".to_string(), vec![r("い", "i")], vec![2]),
            ]
        );
    }

    #[test]
    fn a_short_answer_leaves_the_lines_it_did_not_cover_unanswered() {
        let mut batch = RubyBatch::new();
        batch.add("あ", 1);
        batch.add("い", 2);
        let call = batch.drain(100).pop().unwrap();
        let answered = call.answer(vec![vec![p("あ")]]);
        assert_eq!(answered.len(), 1);
        assert_eq!(answered[0].0, "あ");
        // An answer longer than the question is cut the same way.
        let mut batch = RubyBatch::new();
        batch.add("あ", 1);
        let call = batch.drain(100).pop().unwrap();
        assert_eq!(call.answer(vec![vec![p("あ")], vec![p("い")]]).len(), 1);
    }

    fn r(text: &str, reading: &str) -> RubySpan {
        RubySpan::with_reading(text, reading)
    }

    fn p(text: &str) -> RubySpan {
        RubySpan::plain(text)
    }

    fn kw(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    fn piece(text: &str, reading: Option<&str>, emphasized: bool) -> Piece {
        Piece {
            text: text.into(),
            reading: reading.map(Into::into),
            emphasized,
        }
    }

    #[test]
    fn without_keywords_nothing_is_marked() {
        let spans = [r("日", "にち"), p("は")];
        assert_eq!(
            mark(&spans, &[]),
            [piece("日", Some("にち"), false), piece("は", None, false)]
        );
        // An empty keyword matches nothing (and cannot loop).
        assert_eq!(mark(&spans, &kw(&[""])).len(), 2);
    }

    #[test]
    fn a_keyword_inside_plain_text_cuts_the_span_at_its_edges() {
        let got = mark(&[p("今夜レイドします")], &kw(&["レイド"]));
        assert_eq!(
            got,
            [
                piece("今夜", None, false),
                piece("レイド", None, true),
                piece("します", None, false)
            ]
        );
    }

    #[test]
    fn a_keyword_that_runs_across_spans_marks_every_span_it_touches() {
        // 募集 (with its reading) then plain ます: the keyword 募集ま covers
        // the first whole and the start of the second.
        let got = mark(&[r("募集", "ぼしゅう"), p("します")], &kw(&["募集し"]));
        assert_eq!(
            got,
            [
                piece("募集", Some("ぼしゅう"), true),
                piece("し", None, true),
                piece("ます", None, false)
            ]
        );
    }

    #[test]
    fn a_keyword_inside_a_reading_span_marks_all_of_that_span() {
        let got = mark(&[p("の"), r("辞書", "じしょ")], &kw(&["辞"]));
        assert_eq!(
            got,
            [
                piece("の", None, false),
                piece("辞書", Some("じしょ"), true)
            ]
        );
    }

    #[test]
    fn every_occurrence_is_marked_and_the_earliest_keyword_wins() {
        let got = mark(&[p("abXYab")], &kw(&["ab", "XY"]));
        assert_eq!(
            got,
            [
                piece("ab", None, true),
                piece("XY", None, true),
                piece("ab", None, true)
            ]
        );
        // Both match at the start: the one listed first.
        let got = mark(&[p("abc")], &kw(&["a", "ab"]));
        assert_eq!(got[0], piece("a", None, true));
    }

    #[test]
    fn the_pieces_always_read_back_as_the_spans() {
        let spans = [
            p("お"),
            r("疲", "つか"),
            p("れ"),
            r("様", "さま"),
            p("です"),
        ];
        let line: String = spans.iter().map(|s| s.text.as_str()).collect();
        for words in [
            vec![],
            kw(&["れ様"]),
            kw(&["です", "お"]),
            kw(&["疲れ様です"]),
        ] {
            let got: String = mark(&spans, &words)
                .iter()
                .map(|x| x.text.as_str())
                .collect();
            assert_eq!(got, line, "{words:?}");
        }
    }

    #[test]
    fn the_cache_returns_what_it_was_given_and_forgets_the_oldest() {
        let mut cache = RubyCache::new(2);
        assert!(cache.is_empty() && cache.get("a").is_none());
        cache.put("a", vec![p("a")]);
        cache.put("b", vec![p("b")]);
        assert_eq!(cache.get("a"), Some(&vec![p("a")]));
        cache.put("c", vec![p("c")]);
        assert_eq!(cache.len(), 2);
        assert!(cache.get("a").is_none(), "the oldest left");
        assert!(cache.get("b").is_some() && cache.get("c").is_some());
    }

    #[test]
    fn putting_a_line_again_replaces_it_without_using_a_second_slot() {
        let mut cache = RubyCache::new(2);
        cache.put("a", vec![p("1")]);
        cache.put("a", vec![p("2")]);
        cache.put("b", vec![p("b")]);
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get("a"), Some(&vec![p("2")]));
    }
}
