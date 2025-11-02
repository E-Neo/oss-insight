use std::io::{BufRead, Lines, StdinLock};

pub enum StdinOrIter<I> {
    Stdin(Lines<StdinLock<'static>>),
    Iter(I),
}

impl<'a, I> Iterator for StdinOrIter<I>
where
    I: Iterator<Item = &'a String>,
{
    type Item = std::io::Result<String>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            StdinOrIter::Stdin(lines) => lines.next(),
            StdinOrIter::Iter(lines) => lines.next().map(|s| Ok(s.clone())),
        }
    }
}

pub fn stdin_or_iter<'a, I>(stdin: bool, iter: I) -> StdinOrIter<I::IntoIter>
where
    I: IntoIterator<Item = &'a String>,
{
    if stdin {
        StdinOrIter::Stdin(std::io::stdin().lock().lines())
    } else {
        StdinOrIter::Iter(iter.into_iter())
    }
}
