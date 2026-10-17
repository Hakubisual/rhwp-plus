# 결함 분석: 누름틀 컨트롤('spct')이 HWPX 저장에서 소실

## 요약

공공 별지서식 코퍼스 69개 실측에서 41·43·44·45-lawform.hwp가 `render-diff --json`에서
STRUCT_MISMATCH로 판정됐다. 원인을 추적한 결과:

1. 누름틀(체크박스/라디오) 컨트롤의 HWP5 CTRL_HEADER ctrl_id는 `spct`(0x74637073)이다.
   rhwp 파서의 컨트롤 dispatch에 `spct`가 없어 `Control::Unknown`으로 분류된다.
2. `Control::Unknown`은 HWPX 직렬화에서 의도적으로 버려진다(`section.rs`의
   `occupies_hwpx_slot_axis`/`emits_hwpx_slot_xml` 계약 — Unknown은 슬롯이 아니라고 규정).
3. Unknown이 버려지면 문단의 위치 축(8유닛 슬롯)이 어긋나고, 이후 문단·개체 배치가
   원본과 달라진다. 실측: 원본 렌더 트리의 흰 사각형(체크박스 배경) 6개가 저장본 재파싱
   렌더에서 0개가 된다(43-lawform).

## 근거

- dump-records: `CTRL_HEADER 73 70 63 74 ...` 27건 (43-lawform)
- 저장본 43test.hwpx section0.xml: `hp:rect/ellipse/line/container` 0건
- `hwp5-roundtrip`은 PASS(IR 존재·HWP5 재직렬화 무손실) — HWPX 직렬화 경로만 문제
- 제네릭 셀 경로 단위 테스트(`cell_paragraph_emits_group_shape_children`)는 PASS —
  셀 경로 자체는 정상, 파서 수용 실패가 근원

## 제안 수정

`src/parser/control.rs`의 컨트롤 dispatch에 `spct`를 추가해 `Control::Form`으로
읽는다. `serializer/hwpx/form.rs`의 `write_form`이 이미 `hp:checkBtn`/`hp:radioBtn`을
방출하므로 직렬화 측 작업은 최소이다. 왕복 검증은 기존 `hwpx-roundtrip` 게이트로 커버.

## 관련

- 표 RowBreak 분할 overflow(30건)는 상류 PR #7965가 이미 다룬다 — 중복 수정 금지.
