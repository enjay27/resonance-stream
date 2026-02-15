# 🛠️ Resonance Stream 빌드 가이드

이 문서는 **Resonance Stream**의 소스 코드를 컴파일하고 배포용 패키지로 만드는 전체 프로세스를 안내합니다. 이 프로젝트는 Rust(Tauri) 백엔드와 Python(AI Engine) 사이드카가 결합된 구조이므로 각 단계의 순서가 중요합니다.

## 📋 1. 사전 요구 사항

빌드 전, 다음 도구들이 시스템에 설치되고 경로(PATH)에 등록되어 있어야 합니다.

* **Rust**: 버전 1.70+ (2021 에디션).
* **Node.js & Trunk**: **Leptos** 프론트엔드 빌드 및 번들링용.
* **Python 3.10+**: `ctranslate2`, `pykakasi`, `argparse` 설치 필수.
* **CUDA Toolkit**: NVIDIA GPU 가속을 사용하려면 11.x 또는 12.x 버전이 필요합니다.
* **SDK 파일 배치**:
* **WinDivert SDK**: `WinDivert.dll`, `WinDivert.lib`, `WinDivert64.sys`를 `src-tauri/bin/` 폴더에 수동으로 복사해야 합니다.
* **Npcap SDK**: 패킷 캡처 기능을 위해 필요합니다.



---

## ⚙️ 2. 빌드 환경 설정 (.env)

본 프로젝트는 여러 개발자의 환경이 다를 수 있음을 고려하여, 하드코딩된 경로 대신 `.env` 파일을 통해 로컬 설정을 관리합니다.

1. **환경 변수 파일 생성**: 프로젝트 루트에 있는 `.env.template` 파일을 복사하여 `.env` 파일을 생성합니다.
2. **SDK 경로 설정**: 생성한 `.env` 파일을 열어 본인의 PC에 설치된 Npcap SDK 경로를 입력합니다.
```text
# 예시: NPCAP_PATH=C:\Users\Admin\Downloads\npcap-sdk-1.16
NPCAP_PATH=본인의_SDK_경로
```


*이 설정은 `package.bat` 실행 시 자동으로 로드되어 Rust 링커에 전달됩니다.*

---

## 🚀 3. 컴파일 파이프라인 (단계별 안내)

Tauri 빌드 프로세스 중에 사이드카가 포함되어야 하므로 반드시 **Python 사이드카를 먼저 빌드**해야 합니다.

### 단계 1: AI 사이드카 빌드 (Python)

PyInstaller를 사용하여 AI 엔진을 독립 실행 파일로 만듭니다. 이때 Tauri의 규칙에 따라 **Target Triple 접미사**를 반드시 유지해야 합니다.

* **명령어**: `.spec` 파일을 사용하여 최적화된 설정을 적용합니다.
```powershell
pyinstaller --noconfirm --clean translator-x86_64-pc-windows-msvc.spec
```


* **결과물**: `src-tauri/bin/translator-x86_64-pc-windows-msvc.exe`가 생성됩니다.

### 단계 2: 메인 애플리케이션 빌드 (Rust/Tauri)

프론트엔드 자산을 번들링하고 Rust 백엔드를 컴파일합니다.

* **명령어**:
```bash
cargo tauri build
```


* **참고**: 이 과정에서 `models.json`이 리소스로 포함되며, 사이드카 역시 바이너리에 포함됩니다.

---

## 📦 4. 자동 패키징 및 배포

모든 수동 단계를 생략하고 즉시 배포판을 만들려면 **`package.bat`**를 사용하세요. 이 스크립트는 다음 작업을 수행합니다:

1. `.env` 파일에서 로컬 경로 로드 및 검증.
2. `translator.spec`을 통한 AI 엔진 컴파일.
3. Tauri 앱 전체 빌드.
4. **클린 업(Clean Up)**: 기술적인 이름(`translator-x86_64...`)을 사용자가 보기 편한 **`translator.exe`**로 변경하여 배포 폴더에 수집.
5. 최종 결과물을 `.zip` 파일로 압축.

---

## ⚠️ 5. 신입 개발자 주의 사항

* **모델 관리**: 빌드 후 첫 실행 시, 앱은 `models.json`에 정의된 링크를 참조하여 AI 모델을 다운로드합니다.
* **관리자 권한**: 네트워크 패킷 스니핑을 위한 WinDivert 드라이버 구동을 위해 반드시 **관리자 권한으로 실행**해야 합니다.
* **PathResolver 오류**: 개발 모드(`cargo tauri dev`)에서 `models.json`을 찾지 못하는 경우, 프로젝트 루트에 해당 파일이 있는지, `tauri.conf.json`의 `resources`에 등록되어 있는지 확인하세요.

---
