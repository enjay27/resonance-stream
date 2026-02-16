# 🛠️ Resonance Stream 빌드 가이드

이 문서는 **Resonance Stream**의 소스 코드를 컴파일하고 배포용 패키지로 만드는 전체 프로세스를 안내합니다. 이 프로젝트는 Rust(Tauri) 백엔드와 Python(AI Engine) 사이드카가 결합된 구조이므로 각 단계의 순서가 중요합니다.

## 📋 1. 사전 요구 사항

빌드 전, 다음 도구들이 시스템에 설치되고 경로(PATH)에 등록되어 있어야 합니다.

* **Rust**: 버전 1.70+ (2021 에디션).
* **Node.js & Trunk**: **Leptos** 프론트엔드 빌드 및 번들링용.
* **Python 3.10+**: `ctranslate2`, `pykakasi`, `argparse`, `pyinstaller` 설치 필수.
* **CUDA Toolkit**: NVIDIA GPU 가속을 사용하려면 11.x 또는 12.x 버전이 필요합니다.

---

## ⚙️ 2. 라이브러리 및 의존성 설정 (자동화)

이전 버전과 달리, **WinDivert**와 **Npcap SDK**를 수동으로 다운로드하거나 `.env`에 경로를 지정할 필요가 없습니다. 프로젝트 루트에 포함된 설정 스크립트가 이를 자동으로 처리합니다.

1. **자동 설정 스크립트 실행**:
   프로젝트 루트에서 `setup_libs.bat`를 실행하세요.
   ```cmd
   setup_libs.bat
   ```
- 이 스크립트는 `lib/` 폴더를 생성하고 필요한 SDK(Npcap, WinDivert)를 자동으로 다운로드 및 압축 해제합니다.
- `.gitignore`에 의해 `lib/` 폴더는 버전 관리에서 제외됩니다.

---

## 🚀 3. 컴파일 파이프라인 (단계별 안내)

`package.bat`를 사용하지 않고 수동으로 빌드할 경우, 다음 순서를 반드시 준수해야 합니다.

### 단계 1: 라이브러리 경로 설정

Rust 링커가 Npcap 라이브러리를 찾을 수 있도록 환경 변수를 임시로 설정합니다.
```dos
set LIB=%CD%\lib\npcap-sdk\Lib\x64;%LIB%
```

### 단계 2: AI 사이드카 빌드 (Python)

PyInstaller를 사용하여 AI 엔진을 독립 실행 파일로 만듭니다. Tauri는 사이드카 파일명에 **Target Triple(플랫폼 식별자)**이 포함되어 있어야만 인식합니다.

* **명령어**: `.spec` 파일을 사용하여 최적화된 설정을 적용합니다.
```
pyinstaller --noconfirm --clean --distpath src-tauri\bin translator.spec
```
- **결과물**: `src-tauri/bin/translator-x86_64-pc-windows-msvc.exe` 생성.

### 단계 3: 드라이버 배치

`lib` 폴더에 다운로드된 WinDivert 드라이버를 실행 파일 위치로 복사합니다.

- `lib\WinDivert\x64\WinDivert.dll` -> `src-tauri\bin\`
- `lib\WinDivert\x64\WinDivert64.sys` -> `src-tauri\bin\`

### 단계 4: 메인 애플리케이션 빌드 (Rust/Tauri)

프론트엔드 자산을 번들링하고 Rust 백엔드를 컴파일하여 설치 파일을 생성합니다.

* **명령어**:
```bash
cargo tauri build
```

---

## 📦 4. 자동 패키징 및 배포 (권장)

모든 수동 단계를 생략하고 즉시 배포판을 만들려면 **`package.bat`**를 사용하세요. 이 스크립트는 다음 작업을 수행합니다:

1. **의존성 검사**: `lib/` 폴더 확인 후, 없으면 `setup_libs.bat` 자동 실행.
2. **사이드카 빌드**: `translator.spec`을 통해 AI 엔진 컴파일 및 배치.
3. **리소스 배치**: WinDivert 드라이버 자동 복사.
4. **설치 파일 생성**: Tauri 빌드(NSIS) 실행.
5. **배포 정리 (Move & Organize)**: 생성된 설치 파일(`*-setup.exe`)을 프로젝트 루트의 **`dist`** 폴더로 이동하여 즉시 배포 가능한 상태로 만듭니다.

---

## ⚠️ 5. 주의 사항

- **모델 관리**: 앱 설치 후 첫 실행 시, `models.json`에 정의된 링크를 참조하여 AI 모델을 자동으로 다운로드합니다.
- **관리자 권한**: 네트워크 패킷 스니핑(WinDivert)을 위해 앱은 반드시 **관리자 권한**으로 실행되어야 합니다. (생성된 설치 파일은 이를 자동으로 요구하도록 설정되어 있습니다.)
- **사이드카 디버깅**: 만약 번역 기능이 작동하지 않는다면, `translator.spec`의 `console=True` 옵션을 켜고 빌드하여 Python 콘솔 창에서 오류 메시지를 확인하세요.

---
