/// 노트 머리에서 읽은 값입니다.
///
/// 맵은 읽은 차례를 지키는 `(키, 값)` 쌍의 목록이고 키의 형도 그대로 둡니다(`1: a`의 키는 문자열이 아니라 정수).
#[derive(Debug, Clone)]
pub enum Value {
    Null,
    Bool(bool),
    /// 10진수 글로 적은 정수입니다. `0x1F`와 `31`처럼 같은 수는 같은 글이 되도록 정규화합니다.
    Int(String),
    Float(f64),
    Str(String),
    List(Vec<Value>),
    Map(Vec<(Value, Value)>),
    /// 엔진이 값으로 다루지 않는 명시 태그(`!!binary`, `!!set`, `!!timestamp` 따위)로 읽은 값입니다.
    Other,
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) | (Value::Other, Value::Other) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            // 맵 키로 쓰이므로 NaN끼리도 같고, 0.0과 -0.0은 다르다(Java의 Double.equals와 같다).
            (Value::Float(a), Value::Float(b)) => canonical_bits(*a) == canonical_bits(*b),
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::List(a), Value::List(b)) => a == b,
            (Value::Map(a), Value::Map(b)) => a == b,
            _ => false,
        }
    }
}

/// NaN을 한 가지 비트열로 맞춘 부동소수점 비트입니다.
fn canonical_bits(x: f64) -> u64 {
    if x.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        x.to_bits()
    }
}
