//! How the title bar shows the sniffer and translator: a tone (colour) and a
//! short Korean label per state.

use crate::ui_types::{SnifferState, TranslatorState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Ok,
    Busy,
    Error,
    Off,
}

impl Tone {
    /// The status pill. Error and off open help on click, so they look clickable.
    pub fn pill_class(self) -> &'static str {
        match self {
            Tone::Ok => "bg-success/10 text-success border-success/20",
            Tone::Busy => "bg-warning/10 text-warning border-warning/20",
            Tone::Error => {
                "bg-error/15 text-error border-error/30 cursor-pointer hover:bg-error/25"
            }
            Tone::Off => {
                "bg-base-content/5 text-base-content/50 border-base-content/10 cursor-pointer"
            }
        }
    }

    pub fn dot_class(self) -> &'static str {
        match self {
            Tone::Ok => "bg-success",
            Tone::Busy => "bg-warning animate-pulse",
            Tone::Error => "bg-error",
            Tone::Off => "bg-base-content/40",
        }
    }
}

pub fn sniffer_status(state: SnifferState) -> (Tone, &'static str) {
    match state {
        SnifferState::Active => (Tone::Ok, "캡처 중"),
        SnifferState::Error => (Tone::Error, "캡처 오류"),
        SnifferState::Off => (Tone::Off, "캡처 꺼짐"),
        SnifferState::Starting => (Tone::Busy, "캡처 시작 중"),
        SnifferState::Binding => (Tone::Busy, "캡처 연결 중"),
        SnifferState::Pending => (Tone::Busy, "캡처 대기 중"),
    }
}

pub fn translator_status(state: TranslatorState) -> (Tone, &'static str) {
    match state {
        TranslatorState::Active => (Tone::Ok, "번역 중"),
        TranslatorState::Error => (Tone::Error, "번역 오류"),
        TranslatorState::Off => (Tone::Off, "번역 꺼짐"),
        TranslatorState::Starting => (Tone::Busy, "번역 시작 중"),
        TranslatorState::LoadingModel => (Tone::Busy, "모델 불러오는 중"),
        TranslatorState::CatchingUp => (Tone::Busy, "밀린 번역 중"),
        TranslatorState::Restarting => (Tone::Busy, "번역 재시작 중"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_types::{SnifferState, TranslatorState};

    #[test]
    fn every_sniffer_state_has_a_tone_and_a_korean_label() {
        use SnifferState::*;
        for (state, tone, label) in [
            (Active, Tone::Ok, "캡처 중"),
            (Error, Tone::Error, "캡처 오류"),
            (Off, Tone::Off, "캡처 꺼짐"),
            (Starting, Tone::Busy, "캡처 시작 중"),
            (Binding, Tone::Busy, "캡처 연결 중"),
            (Pending, Tone::Busy, "캡처 대기 중"),
        ] {
            assert_eq!(sniffer_status(state), (tone, label), "{state:?}");
        }
    }

    #[test]
    fn every_translator_state_has_a_tone_and_a_korean_label() {
        use TranslatorState::*;
        for (state, tone, label) in [
            (Active, Tone::Ok, "번역 중"),
            (Error, Tone::Error, "번역 오류"),
            (Off, Tone::Off, "번역 꺼짐"),
            (Starting, Tone::Busy, "번역 시작 중"),
            (LoadingModel, Tone::Busy, "모델 불러오는 중"),
            (CatchingUp, Tone::Busy, "밀린 번역 중"),
            (Restarting, Tone::Busy, "번역 재시작 중"),
        ] {
            assert_eq!(translator_status(state), (tone, label), "{state:?}");
        }
    }

    #[test]
    fn only_error_and_off_pills_look_clickable_and_only_busy_dots_pulse() {
        for tone in [Tone::Ok, Tone::Busy, Tone::Error, Tone::Off] {
            let clickable = matches!(tone, Tone::Error | Tone::Off);
            assert_eq!(
                tone.pill_class().contains("cursor-pointer"),
                clickable,
                "{tone:?}"
            );
            assert_eq!(
                tone.dot_class().contains("animate-pulse"),
                tone == Tone::Busy,
                "{tone:?}"
            );
        }
    }
}
