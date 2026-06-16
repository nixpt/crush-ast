use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::highlight::{Highlighter, MatchingBracketHighlighter};
use rustyline::hint::{Hinter, HistoryHinter};
use rustyline::validate::{
    MatchingBracketValidator, ValidationContext, ValidationResult, Validator,
};
use rustyline::{Context, Helper, Result};
use std::borrow::Cow;

pub struct CrushHelper {
    filename_completer: FilenameCompleter,
    validator: MatchingBracketValidator,
    hinter: HistoryHinter,
    highlighter: MatchingBracketHighlighter,
    colored_prompt: String,
}

impl CrushHelper {
    pub fn new() -> Self {
        Self {
            filename_completer: FilenameCompleter::new(),
            validator: MatchingBracketValidator::new(),
            hinter: HistoryHinter {},
            highlighter: MatchingBracketHighlighter::new(),
            colored_prompt: "".to_owned(),
        }
    }

    pub fn set_prompt(&mut self, prompt: String) {
        self.colored_prompt = prompt;
    }
}

impl Helper for CrushHelper {}

impl Completer for CrushHelper {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Result<(usize, Vec<Pair>)> {
        self.filename_completer.complete(line, pos, ctx)
    }
}

impl Hinter for CrushHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, ctx: &Context<'_>) -> Option<String> {
        self.hinter.hint(line, pos, ctx)
    }
}

impl Validator for CrushHelper {
    fn validate(&self, ctx: &mut ValidationContext) -> Result<ValidationResult> {
        self.validator.validate(ctx)
    }

    fn validate_while_typing(&self) -> bool {
        self.validator.validate_while_typing()
    }
}

impl Highlighter for CrushHelper {
    fn highlight<'l>(&self, line: &'l str, pos: usize) -> Cow<'l, str> {
        let keywords = [
            "fn", "let", "if", "else", "while", "for", "return", "true", "false", "null",
        ];
        let mut highlighted = line.to_string();

        for kw in keywords {
            let replacement = format!("\x1b[35m{}\x1b[0m", kw);
            highlighted = highlighted.replace(kw, &replacement);
        }

        let val_highlighted = self.highlighter.highlight(&highlighted, pos);
        Cow::Owned(val_highlighted.into_owned())
    }

    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        default: bool,
    ) -> Cow<'b, str> {
        if default {
            Cow::Borrowed(&self.colored_prompt)
        } else {
            Cow::Borrowed(prompt)
        }
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> Cow<'h, str> {
        Cow::Owned(format!("\x1b[90m{}\x1b[0m", hint))
    }

    fn highlight_char(&self, line: &str, pos: usize) -> bool {
        self.highlighter.highlight_char(line, pos)
    }
}
