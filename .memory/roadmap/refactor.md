# 구조 개선 로드맵 (2026-10 점검)

측정: 2026-10-06, `main` `1478528`(PR #203 병합 직후). 근거·수치·제안 전부는 [`docs/architecture_review.md`](../../docs/architecture_review.md)
(항목 ID W/R/P/A/M)에 있다. 이전 리팩터링(2026-09, 단계 0~5)은 [`refactor-2026-09.md`](refactor-2026-09.md), 실앱 자동 검증 로드맵은 [`test-automation.md`](test-automation.md).

## 모든 단계의 완료 조건

1. **계획 먼저.** 영향 분석(graft `callers` 출력)을 보여 주고 Kade의 확인을 받은 뒤 시작한다(CLAUDE.md, workflow-control).
2. **테스트 먼저.** 실패하는 단위 테스트를 만들고 올바른 이유로 실패하는 것을 본 뒤 코드를 쓴다. 버그 수정은 재현 테스트부터.
   순수 로직은 `crates/core`(또는 ui의 순수 모듈)에서 테스트한다. 테스트할 수 없는 Tauri/Windows 접착 코드는 커밋 본문에 그렇게 적는다.
3. **해당 부분의 게이트:** core/llama/types는 `just core-check`, ui는 `just ui-check`, app은 `just app-cross-check`(Linux, 컴파일만; `test-env` 유무와 `--release --features test-env`까지) + Windows CI의 `just app-check`. `cargo fmt`는 항상.
4. **리팩터링은 동작을 바꾸지 않는다.** `ChatMessage`/`SystemMessage`의 camelCase는 프로토콜이다. 필드 이름을 바꾸면 리팩터링이 아니다. 새 설정 필드는 앱·ui 두 `AppConfig`와 `app_config_full.json` 픽스처에 같은 이름으로.
5. 돌리지 못한 검증은 마지막 커밋에 `NOT VERIFIED: ...`로 적는다(이 환경은 Windows 앱을 링크·실행하지 못한다).
6. **한 단계 = 한 브랜치 = 한 PR**, PR이 병합된 뒤에 다음 단계. 끝날 때 `MEMORY.md`의 *Now*를 갱신한다.

## 다음 로드맵 (2026-10-06 점검 기준)

순서는 위험이 큰 것(앱·데이터 손실) -> 조용한 누락 -> 신뢰 경계 -> 구조 정리 -> 효율. 모든 단계는 Kade의 확인을 받고 시작한다.

- **S0 문서·기억 정리와 실앱 읽기** (M-1~M-4, W-13) - `MEMORY.md`를 색인(약 40줄)으로, 닫힌 항목은 `.memory/`로(현재 287줄/103KB).
  그리고 수동 `bridge-smoke.yml`(workflow_dispatch)을 `main`에서 돌려 **`CS-restart-nodup`, `CP-big-ack`, 건너뛴 5단계**(chat-rules의 `CR-ruby-*`·一人 읽기 K16, persistence의 `CP-fav-*`, translator-stub의 `TS-dict-*`·K8 `<bos>`, 팝업, 다운로드)를 읽는다.
- **S1 앱·데이터를 잃지 않게** (W-1, R-6, W-3, W-5) - S1a `install_swap`을 core로 + 두 번째 이름 바꾸기가 실패하면 되돌리기, S1b AI 서버 압축을 `.part`에 풀고 이름 바꾸기, S1c 내보내기 타임스탬프 `unwrap` 제거 + 패닉 훅·로그 파일.
- **S2 조용한 누락 없애기** (W-4, W-7, W-6, R-4) - S2a 도착 즉시 아카이브, S2b 설정 읽기 실패 백업·`apply_config`의 `Result`·`write_atomic`·`config_version`, S2c 채널 한도 기본값 한 곳(먼저 의도한 값 결정: Local/Beginner 500 vs 1000), S2d 메인 스레드 `netsh`를 `async`로 + 방화벽 확인 중복 제거.
- **S3 관리자 권한 앱의 신뢰 경계** (W-2, W-8, W-9) - S3a 서버 exe/DLL 파일별 SHA-256 검증(또는 ACL), S3b gist 메타데이터 minisign 검증 + 설치 시점 버전 재확인 + 타임아웃/크기 한도, S3c CSP·안 쓰는 셸 권한 제거·`open_browser` 스킴 허용 목록.
- **S4 서비스 수명 소유자** (R-1, R-2, R-3, W-10) - `sniffer_change` 표와 감시 판단을 core로(가짜 시계 테스트), 2초 중복 창을 core로, `Services` 소유자로 호출부를 하나씩.
- **S5 작은 정리와 결함** (W-11, W-12, P-4~P-6, R-7~R-9, A-2.4) - `[P<n>]` 충돌 수정(골든 갱신), "model" 방침 결정, 변이 테스트가 보여 준 구멍 6개를 테스트로, 차단 이벤트 일괄 발행, 파서 `unknown_fields`, 차단 사본, `app_config.rs`/`types` 분할.
- **S6 UI 정리와 효율** (P-1~P-3, R-5, A-4) - `display_limit` 복귀, 줄별 효과·IPC 정리, 큰 뷰 분할. 화면 변경이므로 `ui-preview` 스크린샷 + `cargo tauri dev`(Windows, 관리자). 창 단위 목록은 결과를 보고 결정.
- **S7 정제된 캡처 코퍼스** (A-1.4) - Kade의 실제 캡처 + 닉네임·본문 치환기 -> `capture_replay.rs`로 CI 재생.
- **하지 않는 것:** 두 `AppConfig` 합치기(기록된 결정), 브리지/`test_env`를 core 밖으로, tokio 전면 이식, 파서·텍스트 미세 최적화, UI 클릭 자동화(Kade 2026-10-06). 이유는 보고서 8.4절.

## 이미 끝난 것 (2026-10-06 세션)

- ~~**reqwest 0.11 -> 0.12**~~ **완료** (PR #199) - 취약점 4건 해소(`h2`·`rustls-webpki` 갱신). 실앱: 번역·사전 동기화·즐겨찾기 정상, 모델 다운로드와 업데이트 확인(대화상자 안 뜸)은 못 봄.
- ~~**텍스트 파이프라인 골든 테스트**~~ **완료** (PR #201) - `crates/core/tests/golden_text.rs`, 스냅샷 8개. 후리가나는 K16이 정해질 때까지 제외.
- ~~**주간 변이 테스트**~~ **완료** (PR #202) - `text.rs` 87개: 잡음 75, 놓침 8, 불가 4, 호스트 러너 6분. 놓친 것은 S5의 테스트 거리.
- ~~**브리지 MQTT 패킷 한도**~~ **완료** (PR #203) - `CS-restart-nodup`의 원인(10KiB 기본 한도가 108KB 응답을 막고 재연결). 한도 16MiB + 초과 응답은 오류 ack. 실앱 결과는 S0에서 읽는다.

## 현황 (숫자) - 2026-10-06

| 부분 | 줄(`.rs`) | 비고 |
|---|---|---|
| `crates/core` | 13,843 | `kanji_on_table.rs` 2,982(표), `test_env.rs` 1,324, `bridge.rs` 1,102, `text.rs` 1,048 |
| `crates/types` | 1,261 | 모든 DTO가 한 파일(R-9) |
| `crates/llama` | 137 | |
| `src/` (ui) | 11,095 | `chat_view.rs` 908, `chat_row.rs` 597(함수 하나 약 481줄, 보고) |
| `src-tauri/src` | 5,530 | `app_config.rs` 581, `sniffer/mod.rs` 450, `translator/mod.rs` 442 |
| `runbook/` (파이썬) | 8,501 | 파이프라인 11개 |

테스트: core·llama·types 495개 통과, `#[test]` 개수 core 385 · types 49 · llama 61 · ui 157 · app 21. 골든 8개. 워크플로 9개. `cargo audit` 취약점 0.

## 기능별 권장 요약 (상세는 보고서 6절)

| 기능 | 가장 먼저 할 것 | 단계 |
|---|---|---|
| 캡처·프로토콜 | 소켓 설정 실패마다 `Error` 상태, 서비스 소유자(`Services`) | S4 |
| 번역 | `[P<n>]` 충돌 수정, 변이 테스트가 보여 준 캐시 구멍 6개 | S5 |
| 채팅 저장 | **도착 즉시 저장**(번역이 생기면 두 번째 줄) | S2 |
| 오버레이 UI | `display_limit` 복귀, 후리가나 IPC 일괄 | S6 |
| 설정·즐겨찾기·사전 | 읽기 실패 백업, `config_version`, 채널 한도 한 곳 | S2 |
| 다운로드·업데이트·보안 | 업데이트 되돌리기, 서버 exe 무결성, 메타데이터 서명 | S1, S3 |
| 테스트·QA | 스모크 단계 독립 실행, 실앱 미확인 행 읽기, 시각 회귀 | S0, S6 |
| 운영·관측성 | 로그 파일 + 패닉 훅, "진단 복사" | S1, 이후 |

## 알려진 결함 (아직 안 고침)

- `[P0]`을 채팅에 쓰면 진짜 자리표시자와 충돌한다(`"[P0]火力"` -> `딜러딜러`). 골든 `a_placeholder_typed_in_chat_collides_with_a_real_one_known_defect`가 현재 동작을 고정한다. (S5)
- 새어 나온 `<start_of_turn>model`이 "model" 단어를 남긴다(`model 번역`). 어떻게 되어야 하는지는 미정. (S5)
