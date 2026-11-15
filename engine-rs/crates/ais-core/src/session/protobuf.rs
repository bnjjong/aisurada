//! 스키마 없이 protobuf 바이트를 필드 목록으로 읽는 가장 작은 읽개. Antigravity(agy)의 대화 기록은 SQLite 칸마다 protobuf 메시지를
//! 넣어 두는데 스키마 파일이 공개돼 있지 않아서, 필요한 필드 번호만 직접 짚어 읽는다. 모르는 필드는 건너뛴다.
//! 길이가 모자란 끝은 `tolerant`일 때 있는 만큼만 돌려준다(긴 도구 인자를 앞부분만 가져올 때 쓴다).

const VARINT: u32 = 0;
const FIXED64: u32 = 1;
const LENGTH_DELIMITED: u32 = 2;
const FIXED32: u32 = 5;

/// 필드 하나. 숫자 필드는 `number64`, 길이가 있는 필드는 `bytes`에 값이 들어 있다.
#[derive(Clone, Debug)]
pub struct Field {
    pub number: u32,
    pub wire: u32,
    pub number64: i64,
    pub bytes: Option<Vec<u8>>,
}

impl Field {
    fn is_message(&self) -> bool {
        self.wire == LENGTH_DELIMITED
    }

    /// 길이가 있는 필드를 글로 읽는다. 숫자 필드면 `None`.
    fn text(&self) -> Option<String> {
        if self.wire == LENGTH_DELIMITED {
            self.bytes
                .as_ref()
                .map(|b| String::from_utf8_lossy(b).into_owned())
        } else {
            None
        }
    }
}

fn varint(data: &[u8], pos: &mut usize) -> Option<i64> {
    let mut value: u64 = 0;
    let mut shift = 0u32;
    while *pos < data.len() && shift < 64 {
        let b = data[*pos];
        *pos += 1;
        value |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Some(value as i64);
        }
        shift += 7;
    }
    None
}

/// 메시지를 필드 목록으로 읽는다. 형식이 틀리면 `None`이다.
/// `tolerant`가 참이면 마지막 길이 필드가 모자라도 있는 만큼을 돌려주고, 읽던 자리에서 멈춘다.
pub fn parse(data: &[u8], tolerant: bool) -> Option<Vec<Field>> {
    let mut fields: Vec<Field> = Vec::new();
    let mut pos = 0usize;
    let fail = |fields: Vec<Field>| if tolerant { Some(fields) } else { None };
    while pos < data.len() {
        let Some(key) = varint(data, &mut pos) else {
            return fail(fields);
        };
        let number = ((key as u64) >> 3) as u32;
        let wire = (key & 7) as u32;
        if number == 0 || number > 536_870_911 {
            return if tolerant && !fields.is_empty() {
                Some(fields)
            } else {
                None
            };
        }
        match wire {
            VARINT => {
                let Some(value) = varint(data, &mut pos) else {
                    return fail(fields);
                };
                fields.push(Field {
                    number,
                    wire,
                    number64: value,
                    bytes: None,
                });
            }
            FIXED64 => {
                if pos + 8 > data.len() {
                    return fail(fields);
                }
                pos += 8;
                fields.push(Field {
                    number,
                    wire,
                    number64: 0,
                    bytes: None,
                });
            }
            FIXED32 => {
                if pos + 4 > data.len() {
                    return fail(fields);
                }
                pos += 4;
                fields.push(Field {
                    number,
                    wire,
                    number64: 0,
                    bytes: None,
                });
            }
            LENGTH_DELIMITED => {
                let Some(length) = varint(data, &mut pos) else {
                    return fail(fields);
                };
                if length < 0 || length > i64::from(i32::MAX) {
                    return fail(fields);
                }
                let available = data.len() - pos;
                let length = length as usize;
                if length > available {
                    if !tolerant {
                        return None;
                    }
                    fields.push(Field {
                        number,
                        wire,
                        number64: 0,
                        bytes: Some(data[pos..].to_vec()),
                    });
                    return Some(fields);
                }
                fields.push(Field {
                    number,
                    wire,
                    number64: 0,
                    bytes: Some(data[pos..pos + length].to_vec()),
                });
                pos += length;
            }
            _ => return fail(fields),
        }
    }
    Some(fields)
}

fn first(fields: &Option<Vec<Field>>, number: u32) -> Option<&Field> {
    fields.as_ref()?.iter().find(|f| f.number == number)
}

/// 번호가 같은 첫 길이 필드를 하위 메시지로 읽는다. 없거나 메시지가 아니면 `None`.
pub fn message(fields: &Option<Vec<Field>>, number: u32, tolerant: bool) -> Option<Vec<Field>> {
    let field = first(fields, number)?;
    if !field.is_message() {
        return None;
    }
    parse(field.bytes.as_deref()?, tolerant)
}

/// 번호가 같은 첫 길이 필드를 글로 읽는다. 없으면 `None`.
pub fn text(fields: &Option<Vec<Field>>, number: u32) -> Option<String> {
    first(fields, number)?.text()
}

/// 번호가 같은 첫 숫자 필드. 없으면 `None`.
pub fn number(fields: &Option<Vec<Field>>, number: u32) -> Option<i64> {
    let field = first(fields, number)?;
    (field.wire == VARINT).then_some(field.number64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_fields_and_tolerates_a_cut_end() {
        // 1: 150(varint), 2: "hi", 3: { 1: "x" }
        let data = [
            0x08, 0x96, 0x01, 0x12, 0x02, b'h', b'i', 0x1a, 0x03, 0x0a, 0x01, b'x',
        ];
        let fields = parse(&data, false);
        assert_eq!(number(&fields, 1), Some(150));
        assert_eq!(text(&fields, 2).as_deref(), Some("hi"));
        assert_eq!(text(&message(&fields, 3, false), 1).as_deref(), Some("x"));
        // 3번 필드가 잘렸다: 엄격하면 틀림, 관대하면 있는 만큼
        let cut = &data[..data.len() - 1];
        assert!(parse(cut, false).is_none());
        assert_eq!(text(&parse(cut, true), 2).as_deref(), Some("hi"));
    }
}
