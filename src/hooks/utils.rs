use crate::errors::AliError;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ReplaceToken {
    pub token: String,
    pub value: String,
    pub template: String,
    pub output: String,
}

impl ReplaceToken {
    pub fn replace(&self, s: &str) -> Result<String, AliError> {
        let token = &format!("{} {} {}", "{{", self.token, "}}");

        if !s.contains(token) {
            return Err(AliError::BadHookCmd(format!(
                "template {} does not contains token \"{token}\"",
                self.template
            )));
        }

        Ok(s.replace(token, &self.value))
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
                template: String::from("dummy.conf"),
                output: String::from("dummy.conf"),
            },
            ("{{ PORT }} foo bar {{PORT}}", "3322 foo bar {{PORT}}"),
        ),
        (
            ReplaceToken {
                token: String::from("foo"),
                value: String::from("bar"),
                template: String::from("dummy.conf"),
                output: String::from("dummy.conf"),
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
                template: String::from("dummy.conf"),
                output: String::from("dummy.conf"),
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
