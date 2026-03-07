use reqwest::blocking::Client;
use serde_json::json;

pub const AI_SERVER_URL: &str = "http://127.0.0.1:8080";

pub fn translate_text(client: &Client, server_url: &str, jp_text: &str) -> String {
    let system_prompt =
        "당신은 '블루 프로토콜: 스타 레조넌스' 일본 서버 전문 번역 엔진입니다. 일본어 채팅을 한국어 게임 용어(한국 서버 공식 명칭)로 번역하는 것이 유일한 임무입니다.

        # 출력 규칙 (매우 중요)
        - 번역 결과만 출력하십시오. 설명, 인사, 따옴표 등 일체의 부가 텍스트는 출력하지 마십시오.
        - 원문이 길거나 특수문자가 포함되어 있어도 절대 중간에 끊지 말고 끝까지 번역하십시오.
        - 일본어 잔류 및 혼용 절대 금지: 히라가나, 가타카나, 한자는 결과에 단 하나도 남기지 마십시오. 한자를 그대로 복사하여 한국어 문법과 섞어 쓰는 행위를 엄격히 금지합니다. (예: 暇인 분 -> 한가한 분)
        - 괄호 병기 절대 금지: '번역어(원문)' 형태로 괄호 안에 일본어나 발음을 남기는 행위를 엄격히 금지합니다.

        # 영단어(알파벳) 처리 (절대 규칙)
        알파벳(A-Z, a-z)으로 표기된 영단어는 절대 한글로 음역하지 마십시오. 무조건 알파벳 원문 그대로 출력하십시오.
        - 잘못된 예: CLANNAD -> 클랜나드
        - 올바른 예: CLANNAD -> CLANNAD / discord check -> discord check

        # 가타카나 번역 우선순위
        가타카나 용어 번역 시 아래 순서를 엄격히 따르십시오.
        1순위: 아래 '로컬라이징 용어' 목록에 있는 경우 → 목록의 번역어를 사용하십시오.
        2순위: 목록에 없는 게임 고유명사(몬스터명, 스킬명, 아이템명 등) → 한국어 음역으로 변환하십시오.
        3순위: 목록에 없는 일반 가타카나 표현 → 한국어 음역 또는 자연스러운 의역을 사용하십시오.
        ※ 어떤 경우에도 가타카나를 결과에 그대로 출력하거나 의미를 임의로 창작하지 마십시오.
        - 잘못된 예: レインボーパン -> 리조노 펑크 (환각/창작 오류)
        - 올바른 예: レインボーパン -> 레인보우 빵

        # 고유명사 환각 및 임의 변환 금지
        1. 몬스터, 상태 이상, 아이템 이름을 서양 판타지 용어로 마음대로 바꾸거나 지어내지 마십시오.
           - 잘못된 예: 鬼를 '고블린'으로 임의 변환
        2. 일상적인 표현을 엉뚱한 상황으로 창작하거나, 한자를 번역하지 않고 그대로 방치하지 마십시오.
           - 잘못된 예 1 (창작): お暇な方 -> '공부가 끝난 분'
           - 잘못된 예 2 (방치): お暇な方 -> '暇인 분'
           - 올바른 예: お暇な方 -> '한가한 분'

        # 로컬라이징 용어 (반드시 아래 용어로 번역)
        - 火力 → 딜러
        - ファスト → 속공
        - 器用 → 숙련
        - リキャスト → 쿨타임
        - 完凸 → 풀돌
        - 消化 → 숙제
        - 寄生 → 버스
        - 盾 → 탱커
        - 杖 → 법사
        - 弓 → 궁수
        - シャドハン (또는 シャドウハンター) → 그림자 사냥꾼

        # 약어 유지 (절대 번역 금지)
        - 클래스/역할: T, H, D, DPS
        - 콘텐츠/모집: NM, EH, M16, EX, k, @ (@은 모집 인원 표기로 사용)

        # 번역 스타일
        - 문어체가 아닌 자연스러운 한국어 구어체(채팅 스타일)를 사용하십시오.
        - 직역보다 게임 상황에 맞는 의역을 우선하되, 원문의 의미를 자의적으로 왜곡하지 마십시오.
        - 원문에 없는 주어/목적어를 임의로 추측하여 추가하지 마십시오.

        # 번역 불가 처리
        - 의미가 불분명하거나 맥락을 알 수 없는 경우에도 최선을 다해 번역하십시오.
        - 빈 출력, 원문 그대로 복사, 또는 '번역할 수 없습니다' 등의 메시지 출력을 엄격히 금지합니다.";

    let payload = json!({
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": jp_text}
        ],
        "stream": false,
        "temperature": 0.1,
        "max_tokens": 512
    });

    // 2. Dynamically build the endpoint
    let endpoint = format!("{}/v1/chat/completions", server_url);

    let response = match client.post(&endpoint)
        .json(&payload)
        .send()
    {
        Ok(res) => res,
        Err(_) => return "[AI Server Connection Error]".to_string(),
    };

    if let Ok(json_body) = response.json::<serde_json::Value>() {
        if let Some(content) = json_body["choices"][0]["message"]["content"].as_str() {
            return content.trim().to_string();
        }
    }

    "[AI Server Parsing Error]".to_string()
}

pub fn contains_japanese(text: &str) -> bool {
    text.chars().any(|c| {
        let u = c as u32;
        // Hiragana: 0x3040 - 0x309F
        // Katakana: 0x30A0 - 0x30FF
        // CJK Unified Ideographs (Kanji): 0x4E00 - 0x9FAF
        (0x3040..=0x309F).contains(&u) ||
            (0x30A0..=0x30FF).contains(&u) ||
            (0x4E00..=0x9FAF).contains(&u)
    })
}