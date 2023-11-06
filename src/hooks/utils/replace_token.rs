use serde_json::json;

use crate::errors::AliError;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ReplaceToken {
    /// Token to find and replace
    /// If `token` is `foo` then it gets expanded to `{{ foo }}`
    pub token: String,
    /// Value to be replaced with
    pub value: String,
}

impl ReplaceToken {
    pub(crate) fn replace(&self, s: &str) -> Result<String, AliError> {
        let token = &format!("{} {} {}", "{{", self.token, "}}");

        if !s.contains(token) {
            return Err(AliError::BadHookCmd(format!(
                "template does not contains token \"{token}\"",
            )));
        }

        Ok(s.replace(token, &self.value))
    }
}

impl ToString for ReplaceToken {
    fn to_string(&self) -> String {
        json!({
            "token": self.token,
            "value": self.value,
        })
        .to_string()
    }
}

#[test]
fn test_replace_token() {
    use std::collections::HashMap;

    let tests = HashMap::from([
        (
            ReplaceToken {
                token: String::from("PORT"),
                value: String::from("3322"),
            },
            ("{{ PORT }} foo bar {{PORT}}", "3322 foo bar {{PORT}}"),
        ),
        (
            ReplaceToken {
                token: String::from("foo"),
                value: String::from("bar"),
            },
            (
                "{{ bar }} {{ foo }} {{ bar }} foo <{{ foo }}>",
                "{{ bar }} bar {{ bar }} foo <bar>",
            ),
        ),
        (
            ReplaceToken {
                token: String::from("foo"),
                value: String::from("bar"),
            },
            (
                "{ foo } {{ foo }} {{ foo }_} foo bar {{{ foo }}} {{ foo {{ foo }}}}",
                "{ foo } bar {{ foo }_} foo bar {bar} {{ foo bar}}",
            ),
        ),
    ]);

    for (replace, (template, expected)) in tests {
        let actual = replace
            .replace(template)
            .expect("failed to replace template {template}");

        assert_eq!(expected, actual);
    }
}
