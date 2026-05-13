use serde_json::Value;

pub fn deep_merge(left: &mut Value, right: Value) {
    if let Value::Object(left) = left
        && let Value::Object(right) = right
    {
        for (k, v) in right {
            if v.is_null() {
                left.remove(&k);
            } else {
                deep_merge(left.entry(k).or_insert(Value::Null), v);
            }
        }

        return;
    }

    *left = right;
}
