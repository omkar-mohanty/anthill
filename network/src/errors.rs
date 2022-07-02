use std::fmt;

#[derive(Debug, Copy, Clone)]
pub enum TriError<A, B, C> {
    A(A),
    B(B),
    C(C),
}

impl<A, B, C> fmt::Display for TriError<A, B, C>
where
    A: fmt::Display,
    B: fmt::Display,
    C: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TriError::A(a) => a.fmt(f),
            TriError::B(b) => b.fmt(f),
            TriError::C(c) => c.fmt(f),
        }
    }
}

impl<A, B, C> std::error::Error for TriError<A, B, C>
where
    A: std::error::Error,
    B: std::error::Error,
    C: std::error::Error,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TriError::A(a) => a.source(),
            TriError::B(b) => b.source(),
            TriError::C(c) => c.source(),
        }
    }
}
