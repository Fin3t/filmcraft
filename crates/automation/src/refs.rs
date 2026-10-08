//! References to earlier results in multi-command runs (`filmcraft-cli run`, MCP
//! `command_batch`): a string param `"$N"` or `"$N.key.0.key"` is step N's result (1-based) or a
//! value inside it, e.g. `{"clip": "$1.clip"}`. The same notation as EffectCraft's batches. A
//! string that starts with `$$` is literal text with one `$` (`"$$5.00"` is `"$5.00"`).

use serde_json::Value;

/// Resolve a `$N` / `$N.key.0.key` reference against the results so far; `None` when `s` is not
/// a reference (an ordinary string).
fn reference(s: &str, results: &[Value]) -> Option<Result<Value, String>> {
    let rest = s.strip_prefix('$')?;
    let mut parts = rest.split('.');
    let n: usize = parts.next()?.parse().ok()?;
    let path: Vec<&str> = parts.collect();
    if path.iter().any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')) {
        return None;
    }
    let Some(mut v) = n.checked_sub(1).and_then(|i| results.get(i)) else {
        return Some(Err(format!("`{s}` refers to step {n}, but only {} step(s) ran before", results.len())));
    };
    for p in &path {
        let next = match (v, p.parse::<usize>()) {
            (Value::Array(a), Ok(i)) => a.get(i),
            (Value::Object(o), _) => o.get(*p),
            _ => None,
        };
        match next {
            Some(x) => v = x,
            None => return Some(Err(format!("`{s}`: step {n} returned {v}, which has no `{p}`"))),
        }
    }
    Some(Ok(v.clone()))
}

/// Replace every `$N…` string in `v` (recursively) with the referenced result.
pub fn substitute(v: &Value, results: &[Value]) -> Result<Value, String> {
    Ok(match v {
        Value::String(s) if s.starts_with("$$") => Value::String(s[1..].to_string()),
        Value::String(s) => match reference(s, results) {
            Some(r) => r?,
            None => v.clone(),
        },
        Value::Array(a) => Value::Array(a.iter().map(|x| substitute(x, results)).collect::<Result<_, _>>()?),
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| Ok((k.clone(), substitute(x, results)?))).collect::<Result<_, String>>()?),
        _ => v.clone(),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::substitute;

    #[test]
    fn references_resolve_nested_and_leave_other_strings() {
        let results = [json!({"clip": 7, "ids": [3, 4]}), json!(null)];
        let p = json!({"clip": "$1.clip", "clips": ["$1.ids.1", "$1"], "name": "$ 1 is not a ref", "price": "$$5.00", "n": 2});
        assert_eq!(
            substitute(&p, &results).unwrap(),
            json!({"clip": 7, "clips": [4, {"clip": 7, "ids": [3, 4]}], "name": "$ 1 is not a ref", "price": "$5.00", "n": 2})
        );
        assert!(substitute(&json!("$3.clip"), &results).unwrap_err().contains("step 3"));
        assert!(substitute(&json!("$5.00"), &results).unwrap_err().contains("step 5"), "write `$$5.00` for the text");
        assert!(substitute(&json!("$0"), &results).unwrap_err().contains("step 0"));
        assert!(substitute(&json!("$2.clip"), &results).unwrap_err().contains("has no `clip`"));
        assert!(substitute(&json!("$1.ids.9"), &results).unwrap_err().contains("has no `9`"));
    }
}
