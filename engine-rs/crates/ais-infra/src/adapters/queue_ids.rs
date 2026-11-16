//! 정해 둔 난수 목록과 운영체제 난수. 환경 변수 `AISURADA_TEST_RANDOM`(쉼표로 가른 8자리 16진수 목록)이 있으면 그 값을 차례로 쓰고,
//! 없거나 다 쓰면 무작위 값이다(Java 엔진의 `aisurada.test.random`과 같은 뜻이라, 같은 값을 주면 두 엔진이 같은 id를 낸다).
//! 시험용 장치이므로 환경 변수를 안 주면 아무 일도 하지 않는다. 목록은 프로세스에 하나다.

use std::collections::VecDeque;
use std::fs::File;
use std::io::Read;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ports::IdSource;
use crate::text::trim;

/// 정해 둔 난수 목록. `None`이면 아직 환경 변수를 읽지 않았다.
static RANDOM_QUEUE: Mutex<Option<VecDeque<String>>> = Mutex::new(None);

pub struct QueueIds;

impl IdSource for QueueIds {
    fn next_random_hex(&self) -> String {
        {
            let mut guard = RANDOM_QUEUE.lock().unwrap();
            let queue = guard.get_or_insert_with(read_queue);
            if let Some(next) = queue.pop_front() {
                return next;
            }
        }
        random_bytes::<4>()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    fn next_random_u64(&self) -> u64 {
        u64::from_be_bytes(random_bytes::<8>())
    }

    fn reset(&self) {
        *RANDOM_QUEUE.lock().unwrap() = Some(read_queue());
    }
}

fn read_queue() -> VecDeque<String> {
    std::env::var("AISURADA_TEST_RANDOM")
        .map(|value| {
            value
                .split(',')
                .map(|part| trim(part).to_lowercase())
                .filter(|part| !part.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buffer = [0u8; N];
    if let Ok(mut file) = File::open("/dev/urandom")
        && file.read_exact(&mut buffer).is_ok()
    {
        return buffer;
    }
    // 운영체제 난수를 못 읽으면 프로세스마다 다른 해시 씨앗과 시각으로 채운다
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    for chunk in buffer.chunks_mut(8) {
        hasher.write_u8(1);
        let bytes = hasher.finish().to_be_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }
    buffer
}
