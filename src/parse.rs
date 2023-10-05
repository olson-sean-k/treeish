#[cfg(feature = "miette")]
use miette::Diagnostic;
use nom::error::VerboseError as NomError;
use std::borrow::Cow;
use std::path::Path;
use thiserror::Error;
use wax::{self, Glob};

use crate::{BuildError, Empty};

type Input<'i> = &'i str;
type ErrorStack<'i> = NomError<Input<'i>>;
type ErrorMode<'i> = nom::Err<ErrorStack<'i>>;

#[derive(Debug, Error)]
#[error("")]
#[cfg_attr(feature = "miette", derive(Diagnostic))]
pub struct ParseError<'t> {
    expression: Cow<'t, str>,
}

impl<'t> ParseError<'t> {
    // TODO: Provide details about parsing in the error.
    fn new(expression: &'t str) -> Self {
        ParseError {
            expression: expression.into(),
        }
    }

    pub fn into_owned(self) -> ParseError<'static> {
        let ParseError { expression } = self;
        ParseError {
            expression: expression.into_owned().into(),
        }
    }
}

#[derive(Debug)]
pub enum Partitioned<'t> {
    Path(Cow<'t, Path>),
    Glob(Glob<'t>),
    GlobIn(Cow<'t, Path>, Glob<'t>),
}

// TODO: Implement escaping of the `::` separator.
pub fn parse(expression: &str) -> Result<Option<Partitioned>, BuildError> {
    use nom::bytes::complete as bytes;
    use nom::{branch, combinator, sequence};

    combinator::all_consuming(branch::alt((
        combinator::map(
            sequence::separated_pair(
                bytes::take_until::<_, _, ErrorStack<'_>>("::"),
                bytes::tag("::"),
                combinator::rest,
            ),
            |(path, glob)| {
                Glob::new(glob).map(|glob| {
                    Some(match Path::new(path).non_empty() {
                        Some(path) => Partitioned::GlobIn(path.into(), glob),
                        _ => Partitioned::Glob(glob),
                    })
                })
            },
        ),
        combinator::map(combinator::rest, |expression| {
            Glob::new(expression)
                .map(|glob| {
                    glob.non_empty().map(|glob| {
                        // There is no `::` separator. Attempt to parse a glob expression, but prefer
                        // emitting native paths if at all possible.
                        let (path, glob) = glob.partition();
                        match (path.non_empty(), glob.non_empty()) {
                            (Some(path), Some(glob)) => Partitioned::GlobIn(path.into(), glob),
                            (None, Some(glob)) => Partitioned::Glob(glob),
                            (Some(path), None) => Partitioned::Path(path.into()),
                            (None, None) => unreachable!(),
                        }
                    })
                })
                .or_else(|_| {
                    Ok(Path::new(expression)
                        .non_empty()
                        .map(|path| Partitioned::Path(path.into())))
                })
        }),
    )))(expression)
    .map(|(_, treeish)| treeish.map_err(From::from))
    .unwrap_or_else(|_: ErrorMode| {
        // TODO: Provide details about parsing in the error.
        Err(ParseError::new(expression).into_owned().into())
    })
}

#[cfg(test)]
mod tests {}
