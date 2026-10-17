//! Markdown → `ScaffoldSpec` 변환 — `rhwp import-markdown` 의 핵심이다.
//!
//! upstream 에 `export-markdown`(읽기)만 있고 Markdown **가져오기**(쓰기)는 hwp-mcp·
//! claw-hwp 를 포함해 어디에도 없다. 외부 파서 의존성을 더하지 않고(GFM 파이프 표·ATX
//! 제목·목록·인용·코드펜스) 스키마 `scaffold_schema_v1` 의 `Block` 으로 내려보낸 뒤,
//! `scaffold` 와 동일한 검증된 생성 경로로 HWPX 를 만든다.
//!
//! 변환 규칙:
//! - `# ~ #######` → `Block::Heading` (7 초과는 7로 클램프)
//! - GFM 파이프 표(`| a | b |` + `|---|---|`) → `Block::Table` (정렬 행은 소비)
//! - 목록·인용은 접두 기호를 텍스트에 보존한 `Block::Paragraph` — scaffold 스키마가
//!   목록 서식을 갖지 않으므로 정보 손실 대신 가시 보존.
//! - ``` 펜스 → 코드 줄마다 `Block::Paragraph` (들여쓰기 보존)
//! - 인라인 강조(`**`, `*`, `` ` ``, `~~`, 링크 `[t](u)` → `t`)는 평문으로 병합한다 —
//!   스키마가 서식을 갖지 않아 강조를 버리는 것보다 텍스트를 남기는 것이 정보 보존이다.

use crate::scaffold::schema::{Block, ScaffoldSpec};

/// Markdown 문자열을 [`ScaffoldSpec`]으로 변환한다.
pub fn markdown_to_spec(md: &str) -> ScaffoldSpec {
    let mut blocks = Vec::new();
    let lines: Vec<&str> = md.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();

        // 코드 펜스 — 닫는 펜스까지 한 줄씩 문단으로 (들여쓰기 보존).
        if trimmed.starts_with("```") {
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                blocks.push(Block::Paragraph {
                    text: lines[i].to_string(),
                });
                i += 1;
            }
            i += 1; // 닫는 펜스 소비 (문서 끝이면 루프가 끝난다)
            continue;
        }

        // ATX 제목.
        if let Some(level) = atx_heading_level(trimmed) {
            let text = strip_inline(trimmed[level..].trim_start());
            blocks.push(Block::Heading {
                level: level.min(7) as u8,
                text: text.trim().to_string(),
            });
            i += 1;
            continue;
        }

        // GFM 파이프 표: 현재 줄이 파이프 행이고 다음 줄이 구분 행이면 표로 묶는다.
        if is_pipe_row(trimmed) && i + 1 < lines.len() && is_pipe_delimiter(lines[i + 1].trim()) {
            let mut rows: Vec<Vec<String>> = vec![parse_pipe_row(trimmed)];
            i += 2; // 헤더 + 구분 행
            while i < lines.len() && is_pipe_row(lines[i].trim()) {
                rows.push(parse_pipe_row(lines[i].trim()));
                i += 1;
            }
            blocks.push(Block::Table { rows, cell_align: None });
            continue;
        }

        // 빈 줄 — 블록 경계로 소비.
        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        // 인용·목록·문단 — 접두를 텍스트에 보존한 문단 하나.
        let text = strip_inline(trimmed);
        blocks.push(Block::Paragraph {
            text: text.trim().to_string(),
        });
        i += 1;
    }

    ScaffoldSpec {
        version: crate::schema_registry::SCAFFOLD_SCHEMA_VERSION.to_string(),
        title: None,
        font: "함초롬바탕".to_string(),
        page_size: crate::scaffold::schema::PageSize { width_mm: 210.0, height_mm: 297.0 },
        blocks,
    }
}

/// ATX 제목(`#`~`#######`)의 수준. 제목이 아니면 None.
fn atx_heading_level(s: &str) -> Option<usize> {
    let hashes = s.chars().take_while(|c| *c == '#').count();
    if (1..=7).contains(&hashes) && s[hashes..].starts_with(' ') {
        Some(hashes)
    } else {
        None
    }
}

/// 파이프 표의 한 행인지 (|로 시작하고 |로 끝나며 |가 2개 이상).
fn is_pipe_row(s: &str) -> bool {
    s.starts_with('|') && s.ends_with('|') && s.matches('|').count() >= 2
}

/// 구분 행(`|---|:--:|`)인지.
fn is_pipe_delimiter(s: &str) -> bool {
    is_pipe_row(s) && s.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

/// 파이프 행을 셀 텍스트 목록으로. 양끝 | 제거 후 | 분할(이스케이프된 \| 미지원 —
/// scaffold 셀은 평문이므로 원문 보존이 우선).
fn parse_pipe_row(s: &str) -> Vec<String> {
    let inner = &s[1..s.len() - 1];
    inner.split('|').map(|cell| strip_inline(cell.trim())).collect()
}

/// 인라인 강조 표시를 걷는다 — `**t**`, `*t*`, `` `t` ``, `~~t~~`, `[t](u)` → `t`.
fn strip_inline(s: &str) -> String {
    let mut out = s.to_string();
    for mark in ["**", "__", "~~", "`", "*", "_"] {
        loop {
            let Some(first) = out.find(mark) else { break };
            let rest = &out[first + mark.len()..];
            let Some(second) = rest.find(mark) else { break };
            let inner = &rest[..second];
            out = format!("{}{}{}", &out[..first], inner, &rest[second + mark.len()..]);
        }
    }
    // 링크 [t](u) → t — 괄호 쌍이 올바른 위치에 있을 때만.
    if let Some(open) = out.find('[') {
        if let (Some(close_bracket), Some(close_paren)) = (out.find(']'), out.rfind(')')) {
            if close_bracket > open && close_paren > close_bracket {
                let text = out[open + 1..close_bracket].to_string();
                out = format!("{}{}{}", &out[..open], text, &out[close_paren + 1..]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_become_heading_blocks() {
        let spec = markdown_to_spec("# 제목\n\n본문\n### 하위");
        assert_eq!(spec.blocks.len(), 3);
        assert!(matches!(&spec.blocks[0], Block::Heading { level: 1, text } if text == "제목"));
        assert!(matches!(&spec.blocks[1], Block::Paragraph { text } if text == "본문"));
        assert!(matches!(&spec.blocks[2], Block::Heading { level: 3, text } if text == "하위"));
    }

    #[test]
    fn gfm_table_becomes_table_block() {
        let md = "| 이름 | 수량 |\n|---|---|\n| 사과 | 3 |\n| 배 | 1 |";
        let spec = markdown_to_spec(md);
        assert_eq!(spec.blocks.len(), 1);
        let Block::Table { rows, .. } = &spec.blocks[0] else {
            panic!("표 블록이어야 함");
        };
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], vec!["이름".to_string(), "수량".to_string()]);
        assert_eq!(rows[2], vec!["배".to_string(), "1".to_string()]);
    }

    #[test]
    fn inline_emphasis_is_merged_to_text() {
        let spec = markdown_to_spec("**굵게**과 *기울임*, `코드`와 [링크](https://x.y) 그리고 ~~취소~~");
        let Block::Paragraph { text } = &spec.blocks[0] else {
            panic!("문단이어야 함");
        };
        assert!(text.contains("굵게과 기울임, 코드와 링크 그리고 취소"), "{text}");
        assert!(!text.contains('*'));
        assert!(!text.contains('`'));
    }

    #[test]
    fn fence_preserves_indentation() {
        let md = "```\nlet a = 1;\n    들여쓴 줄\n```";
        let spec = markdown_to_spec(md);
        assert_eq!(spec.blocks.len(), 2);
        assert!(matches!(&spec.blocks[1], Block::Paragraph { text } if text == "    들여쓴 줄"));
    }

    #[test]
    fn generated_document_roundtrips_through_hwpx() {
        // 변환 결과가 scaffold 왕복 계약을 통과하는지 — build_scaffold 결과를
        // 직렬화해 되읽어 내용이 복원되는지 검사한다.
        let md = "# 보고서\n\n첫 문단입니다.\n\n| 항목 | 값 |\n|---|---|\n| 가 | 1 |";
        let spec = markdown_to_spec(md);
        // build_scaffold 결과(IR)를 HWPX 로 직렬화해 되읽는 scaffold 왕복 계약으로 검증한다.
        let bytes = crate::serializer::hwpx::serialize_hwpx(&crate::scaffold::build_scaffold(&spec))
            .expect("serialize");
        let core = crate::document_core::DocumentCore::from_bytes(&bytes).expect("reparse");
        let text = core.extract_page_text_native(0).expect("extract text");
        assert!(text.contains("보고서"), "{text}");
        assert!(text.contains("첫 문단입니다"), "{text}");
        assert!(text.contains("항목"), "{text}");
        assert!(text.contains("가"), "{text}");
    }
}
