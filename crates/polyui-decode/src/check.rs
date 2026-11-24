use llguidance::api::TopLevelGrammar;
use llguidance::toktrie::{ApproximateTokEnv, TokEnv, TokenId};
use llguidance::{Matcher, ParserFactory};
use serde_json::Value;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fed {
    pub consumed: usize,
    pub total: usize,
    pub accepting: bool,
}

impl Fed {
    pub fn accepted(&self) -> bool {
        self.consumed == self.total && self.accepting
    }

    pub fn rejected_at(&self) -> Option<usize> {
        (self.consumed < self.total).then_some(self.consumed)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Timing {
    pub compile: Duration,
    pub first_mask: Duration,
    pub warnings: Vec<String>,
}

pub struct Checker {
    factory: ParserFactory,
}

impl Checker {
    pub fn byte_level() -> Self {
        Self::new(&ApproximateTokEnv::single_byte_env())
            .expect("the byte level tokenizer always builds a parser factory")
    }

    pub fn new(tok_env: &TokEnv) -> Result<Self, String> {
        let mut factory = ParserFactory::new_simple(tok_env).map_err(|error| error.to_string())?;
        factory.quiet();
        Ok(Self { factory })
    }

    pub fn tok_env(&self) -> &TokEnv {
        self.factory.tok_env()
    }

    pub fn matcher(&self, grammar: TopLevelGrammar) -> Result<Matcher, String> {
        let parser = self
            .factory
            .create_parser(grammar)
            .map_err(|error| error.to_string())?;
        let matcher = Matcher::new(Ok(parser));
        match matcher.get_error() {
            Some(error) => Err(error),
            None => Ok(matcher),
        }
    }

    pub fn lark(&self, grammar: &str) -> Result<Matcher, String> {
        self.matcher(TopLevelGrammar::from_lark(grammar.to_string()))
    }

    pub fn json_schema(&self, schema: &Value) -> Result<Matcher, String> {
        self.matcher(TopLevelGrammar::from_json_schema(schema.clone()))
    }

    pub fn time(&self, grammar: TopLevelGrammar) -> Result<Timing, String> {
        let started = Instant::now();
        let mut matcher = self.matcher(grammar)?;
        let compile = started.elapsed();
        let started = Instant::now();
        matcher.compute_mask().map_err(|error| error.to_string())?;
        let first_mask = started.elapsed();
        Ok(Timing {
            compile,
            first_mask,
            warnings: matcher.grammar_warnings(),
        })
    }

    pub fn feed_tokens(&self, matcher: &mut Matcher, tokens: &[TokenId]) -> Result<Fed, String> {
        let consumed = matcher
            .try_consume_tokens(tokens)
            .map_err(|error| error.to_string())?;
        let accepting = consumed == tokens.len()
            && matcher.is_accepting().map_err(|error| error.to_string())?;
        Ok(Fed {
            consumed,
            total: tokens.len(),
            accepting,
        })
    }

    pub fn feed(&self, matcher: &mut Matcher, text: &str) -> Result<Fed, String> {
        let tokens = self.factory.tok_env().tokenize_bytes(text.as_bytes());
        self.feed_tokens(matcher, &tokens)
    }
}
