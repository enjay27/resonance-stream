# 🛰️ Resonance Stream

**Blue Protocol: Star Resonance (BPSR) Real-time Packet-Sniffing Translator**

> 일본어 서버의 패킷을 실시간으로 감지하고 AI를 통해 고품질 한국어로 번역하는 정교한 유틸리티입니다.

[English Version](./README_EN.md) | [Troubleshooting (EN)](./TROUBLE_SHOOTING_EN.md)

[Troubleshooting (KO)](./TROUBLE_SHOOTING.md)

---

## 📖 소개 (Introduction)

**Resonance Stream**은 게임 클라이언트를 직접 수정(Hooking)하지 않고, 네트워크 패킷을 스니핑하여 대화 내용을 추출하고 번역합니다. 이를 통해 계정 보안을 유지하면서도 쾌적한 일본어 서버 플레이 환경을 제공합니다. **RTX 4080 Super**와 같은 고사양 GPU를 활용한 강력한 AI 추론 엔진을 탑재하고 있습니다.

### ✨ 핵심 기능 (Key Features)

* **비침습적 방식**: WinDivert를 이용한 패킷 스니핑 방식으로 게임 클라이언트에 영향을 주지 않습니다.
* **고성능 AI 엔진**: CTranslate2 기반의 파이썬 사이드카를 사용하여 저지연 고품질 번역을 수행합니다.
* **최적화된 설계**: 39MB의 초경량 엔진 구성으로 리소스 점유율을 최소화했습니다.
* **성능 티어 시스템**: 사용자 하드웨어 사양에 맞춘 4단계 성능 옵션(Low ~ Extreme)을 제공합니다.
* **지능적 후처리**: 한국어 조사 교정 로직 및 고유 명사 닉네임 관리 기능을 포함합니다.

---

## 🛠️ 설치 및 요구 사항 (Setup & Requirements)

### 필수 구성 요소

1. **관리자 권한**: 패킷 감지를 위해 프로그램 실행 시 관리자 권한이 반드시 필요합니다.
2. **MSVC++ Redistributable (x64)**: 파이썬 엔진 구동을 위해 필수적입니다. [다운로드](https://www.google.com/search?q=https://aka.ms/vs/17/release/vc_redist.x64.exe).
3. **NVIDIA GPU**: CUDA 가속을 통한 최상의 번역 경험을 위해 최신 드라이버 설치를 권장합니다.
4. winget install LLVM.LLVM
    - $env:LIBCLANG_PATH="C:\Program Files\LLVM\bin" 환경변수 등록

### 설치 방법

1. 최신 [Release] 섹션에서 `.zip` 파일을 다운로드합니다.
2. 압축을 풀고 모든 파일(`translator.exe`, `WinDivert64.sys` 등)이 동일 폴더에 있는지 확인합니다.
3. `translator.exe`를 **관리자 권한**으로 실행합니다.

---

## 🗑️ 삭제 안내 (Uninstall Guide)

**Resonance Stream**은 고품질 번역 모델을 위해 약 **1.3GB**의 데이터를 사용합니다. 프로그램을 삭제하실 때 다음 사항을 확인해 주세요.

1. **언인스톨러 실행**: 제어판이나 설치 폴더의 `uninstall.exe`를 실행합니다.
2. **데이터 삭제 옵션 선택 (중요)**:
    - 삭제 창에서 **"Delete the application data"** 항목을 반드시 체크해 주세요.
    - 이 항목을 체크해야 하드 드라이브의 **1.3GB 모델 데이터가 함께 삭제**되어 용량을 확보할 수 있습니다.
3. **수동 확인 (선택 사항)**: 만약 체크를 잊으셨다면, `%LOCALAPPDATA%\Resonance Stream` 폴더를 직접 삭제하시면 됩니다.

---

## 🚀 사용법 (Usage)

1. 앱을 실행한 후 **설정(⚙️)** 탭에서 본인의 GPU 사양에 맞는 **Performance Tier**를 선택합니다.
2. 게임(BPSR)을 실행하고 서버에 접속하면 자동으로 패킷 감지가 시작됩니다.
3. **시스템(⚙️)** 탭에서 `First Packet Captured!` 메시지가 출력되는지 확인하세요.
4. 번역된 내용은 앱의 메인 화면에 실시간으로 표시됩니다.

---

## 📜 버전 히스토리 (Changelog)

### v1.0.0 (2026-02-16) - 정식 출시 (Initial Release)

- **엔진 최적화**: PyTorch 의존성을 완전히 제거하여 사이드카 용량을 39MB로 축소했습니다.
- **실시간 패킷 분석**: WinDivert 기반의 고속 패킷 스니핑 로직을 구현했습니다.
- **AI 성능 티어**: Low, Middle, High, Extreme 등 4단계 성능 옵션을 추가했습니다.
- **한국어 보정**: 자연스러운 번역을 위한 조사(을/를, 이/가) 자동 교정 로직을 도입했습니다.
- **닉네임 관리**: 대화 상대방의 닉네임을 로마자로 변환하여 관리하는 기능을 추가했습니다.
- **진단 시스템**: 실시간 드라이버 및 네트워크 상태 확인을 위한 시스템 로그(Debug Mode)를 지원합니다.

---

## 👤 개발자

* **Enjay** ([kdkyoung@gmail.com](mailto:kdkyoung@gmail.com))

---