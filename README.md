# hanun

Windows 터미널(Windows Terminal / PowerShell / cmd)에서 CPU·메모리·디스크·네트워크·프로세스를
한 화면에 보여주는 단일 exe 리소스 모니터 TUI. 자체 CPU 사용량은 유휴 상태에서 1% 안팎을
목표로 한다.

## 실행

```
cargo run --release
```

또는 빌드된 `hanun.exe`를 직접 실행한다.

```
hanun [OPTIONS]
```

### 옵션

| 옵션 | 설명 |
|---|---|
| `--ascii` | 유니코드 테두리/게이지 대신 ASCII 문자(`+`, `-`, `|`, `#`)를 사용한다. 유니코드 박스 문자가 깨지는 터미널에서 사용. |
| `-h`, `--help` | 사용법을 출력하고 종료한다. |
| `-V`, `--version` | 버전을 출력하고 종료한다. |

## 키 바인딩

| 키 | 동작 |
|---|---|
| `q` / `Esc` | 종료 (필터/도움말 열려 있으면 해당 모드만 닫음) |
| `Tab`, `1`-`5` | 패널 포커스 전환 (CPU/메모리/디스크/네트워크/프로세스) |
| `↑`/`↓`, `j`/`k` | 포커스된 패널 스크롤 |
| `s` | 프로세스 정렬 기준 순환 (CPU → MEM → PID → NAME) |
| `/` | 프로세스 이름 필터 입력 (`Enter`/`Esc`로 종료) |
| `p` | 샘플링 일시정지/재개 |
| `+` / `-` | 갱신 주기 조절 (500ms–5000ms) |
| `?` | 도움말 오버레이 토글 |

최소 지원 화면 크기는 80x24이며, 그보다 작으면 안내 화면이 표시된다.

## 아키텍처

```
src/
  main.rs      터미널 init/restore, panic hook, CLI 인자 파싱, 메인 루프
  app.rs       App 상태, 키 입력 처리(update)
  event.rs     입력/틱/스냅샷 이벤트를 하나의 채널로 통합
  sampler.rs   sysinfo 수집 전용 스레드 → Snapshot 전송
  model.rs     Snapshot, History(링버퍼), 정렬/패널 타입 등 순수 데이터
  ui/          mod.rs, cpu.rs, memory.rs, disk.rs, network.rs, process.rs, help.rs, theme.rs
```

수집(sampler)과 렌더(ui)는 분리되어 있으며, `ui`는 `sysinfo` 타입을 직접 참조하지 않고
`model::Snapshot`만 읽는다. 색 임계값(70%/90%)을 포함한 모든 색상 정의는 `ui/theme.rs`
한 곳에만 있다.

## 개발

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

의존성: `ratatui`(crossterm 백엔드), `sysinfo`, `color-eyre`(에러), `tracing`/`tracing-appender`
(파일 전용 로깅).
