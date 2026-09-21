# Hanun — Windows 리소스 모니터 TUI

## 이름 규칙
- 패키지명과 바이너리명은 `hanun`으로 통일한다 (Cargo.toml의 package.name과 [[bin]] name 일치).
- UI 상단 바, README 제목, --help 문구에서도 같은 이름을 쓴다.

## 목표
Windows 터미널(Windows Terminal / PowerShell / cmd)에서 CPU·메모리·디스크·네트워크·프로세스를
한 화면에 보여주는 단일 exe TUI. 자체 CPU 사용량은 1% 안팎을 목표로 한다.

## 기술 스택
- Rust, ratatui 0.30.x (crossterm 백엔드), sysinfo (최신)
- 에러 처리: color-eyre, 로깅: tracing (파일 출력 전용)
- 의존성은 `cargo add`로 최신을 설치하고 Cargo.lock을 커밋한다.

## 명령어
- 실행: `cargo run`
- 릴리스: `cargo build --release`
- 검사: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`

## 아키텍처
src/
  main.rs      터미널 init/restore, panic hook, 메인 루프
  app.rs       App 상태, 키 입력 처리(update)
  event.rs     입력/틱/스냅샷 이벤트를 하나의 채널로 통합
  sampler.rs   sysinfo 수집 전용 스레드 → Snapshot 전송
  model.rs     Snapshot, History(링버퍼), 레이트 계산 등 순수 데이터/로직
  ui/          mod.rs, cpu.rs, memory.rs, disk.rs, network.rs, process.rs, chart.rs, help.rs, theme.rs

원칙:
- 수집(sampler)과 렌더(ui)는 분리한다. ui는 sysinfo 타입을 직접 참조하지 않고 model::Snapshot만 읽는다.
- draw 함수는 상태를 읽기만 하는 순수 함수로 유지한다.
- 렌더 경로에서 blocking 호출(sleep, I/O, 락 장시간 보유)을 하지 않는다.

## sysinfo 규칙
- `System` 인스턴스는 샘플러에서 하나만 유지하고 재생성하지 않는다.
- 루프 안에서 `refresh_all()` 대신 필요한 항목만 refresh한다.
- CPU 사용률은 이전 refresh와의 차이로 계산되므로, 갱신 간격은
  `MINIMUM_CPU_UPDATE_INTERVAL` 이상으로 하고 첫 샘플은 0으로 취급한다.
- 네트워크 속도 = (현재 누적 − 이전 누적) / 경과 시간. 경과 시간은 Instant로 측정한다.
- 프로세스 CPU%가 100%×코어 수까지 나올 수 있으므로 표시 시 정규화 정책을 명시한다.
- 플랫폼에서 값이 없을 수 있는 항목(온도 등)은 Option으로 다루고 UI에 "N/A"로 표시한다.

## Windows 규칙
- crossterm 키 이벤트는 `KeyEventKind::Press`만 처리한다 (Windows는 Release도 전달됨).
- 정상 종료, 에러, 패닉 모두에서 raw mode와 alternate screen을 복구한다 (panic hook 필수).
- 리사이즈 이벤트에 대응하고, 최소 80x24 미만이면 안내 문구를 표시한다.
- 색상은 기본 16색으로도 읽히게 하고, 유니코드 블록 문자가 깨지는 환경을 고려해 ASCII 대체 옵션을 둔다.

## UX 규칙
- 키: q/Esc 종료 · Tab 또는 1–6 패널 포커스 · ↑↓/jk 스크롤 · s 정렬 전환 · / 필터
  · p 일시정지 · +/- 갱신 주기 · g 차트 종류 전환(메모리/디스크/CPU) · ? 도움말
- 기본 갱신 주기 1000ms, 조절 범위 500–5000ms.
- 색 임계값: 사용률 70% 이상 노랑, 90% 이상 빨강. 색은 theme.rs 한 곳에서만 정의한다.

## 코드 규칙
- `unwrap()`/`expect()` 금지 (main의 초기화 구간 제외). 에러는 `?`로 전파한다.
- `println!`/`eprintln!` 금지 (TUI 화면이 깨진다). 로그는 tracing으로 파일에 남긴다.
- 함수는 짧게, 모듈은 단일 책임으로. 공개 함수에는 한 줄 doc comment.
- 테스트: model은 단위 테스트, ui는 ratatui `TestBackend` 기반 렌더 테스트.

## 작업 방식
- 큰 작업은 먼저 계획을 제시하고 승인받은 뒤 구현한다.
- 각 단계 종료 시 fmt/clippy/test 통과를 확인하고 "변경 파일 · 실행 방법 · 남은 위험"을 요약한다.
- 요청하지 않은 기능은 추가하지 않는다. 의존성 추가 시 이유를 먼저 설명한다.
- API 시그니처가 불확실하면 추측하지 말고 컴파일 결과와 `cargo doc`로 확인한다.
- 프로세스 종료(kill) 같은 파괴적 기능은 확인 대화상자 없이 구현하지 않는다.

## Windows 규칙 (추가)
- 터미널 초기화는 각 단계가 성공할 때마다 복구 대상을 기록하고, 중간에 실패해도
  이미 성공한 단계를 되돌린다. 복구 함수는 여러 번 호출해도 안전해야 한다.

## 성능 규칙 (추가)
- 정렬·필터 결과는 입력이 바뀔 때만 재계산하고 Tick마다 다시 계산하지 않는다.
- 렌더링 경로에서 컬렉션 복사, 문자열 할당은 화면에 보이는 행에 한정한다.
- 성능 개선은 측정값(전/후)을 근거로 하며, 측정 없이 "최적화"만 하지 않는다.

## 아키텍처 (추가)
- 정규화, 레이트 계산, 정렬, 필터는 model에만 둔다. sampler와 ui에는 두지 않는다.