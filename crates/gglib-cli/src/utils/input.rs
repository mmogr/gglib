//! What a command asks a person, by one rule: a question is asked only of a
//! terminal.
//!
//! Whether the input is a terminal is asked once, by [`Input::stdin`], and
//! handed to the rule as a value, so every test runs without a terminal and
//! on every OS. With a terminal, a question is put and its answer read. With
//! none, the input is never read, and the question comes to its
//! [`Unasked`]: the answer the command can state, taken and said, or a
//! refusal naming the flag that answers it. Nothing waits.
//!
//! Three offers still ask stdin themselves, by the same rule, and are to be
//! moved onto this one: `config::engine`'s install offer (`offer_action`),
//! and `gglib up`'s `require_tty` and its image-runtime offer
//! (`images_action`).

use anyhow::{Context, Result, anyhow, bail};
use std::io::{self, BufRead, Write};

/// Whether a command's input is a terminal: whether there is somebody to
/// ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Input {
    /// A person at a terminal, who sees the question and types the answer.
    Terminal,
    /// A pipe, a file, a closed input: nobody to ask. Never read for an
    /// answer, even when something is waiting in it: one that nobody writes
    /// to and nobody closes, as an SSH session or a backgrounded shell
    /// gives, would be waited on for ever, and nothing tells the two apart
    /// without waiting.
    NotTerminal,
}

impl Input {
    /// Whether stdin is a terminal: the one place a question asks it.
    #[cfg(not(test))]
    pub(crate) fn stdin() -> Self {
        use std::io::IsTerminal as _;
        Self::from_terminal(io::stdin().is_terminal())
    }

    /// In a test, the terminal is a task that set [`TYPED`], and nothing
    /// else is one: whatever the tests were run from is not the input of the
    /// command a test runs.
    #[cfg(test)]
    pub(crate) fn stdin() -> Self {
        Self::from_terminal(TYPED.try_with(|_| ()).is_ok())
    }

    const fn from_terminal(terminal: bool) -> Self {
        if terminal {
            Self::Terminal
        } else {
            Self::NotTerminal
        }
    }
}

/// What a question comes to when there is nobody to ask it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Unasked<T> {
    /// An answer the command can state truthfully, such as the parameter
    /// count the file gives, and the line that says what was taken and why.
    /// A number question shows the same answer as the one Enter takes.
    Take(T, String),
    /// No answer is safe to take, as none is to a removal: nothing is done,
    /// and the command fails with this sentence, which names the flag that
    /// answers the question.
    Refuse(String),
}

/// The sentence a question nobody can answer is refused with: what was not
/// done, and the flag that does it, with what it does
/// (`nobody_to_ask("the model was not removed", "--yes to remove it")`).
pub(crate) fn nobody_to_ask(not_done: &str, again: &str) -> String {
    format!("There is no terminal to ask, so {not_done}: run it again with {again}.")
}

/// The rule every question is asked by: with a terminal, `ask` it, writing
/// on `said_on`; with none, read nothing and come to `unasked`, a taken
/// answer's line written on `said_on`.
///
/// The one place that decides whether an input that is not a terminal is
/// read. A question asked some other way (a word to type, a question on
/// stderr) is still asked through this, its own reading as `ask`.
///
/// # Errors
///
/// [`Unasked::Refuse`]'s sentence with nobody to ask; otherwise what `ask`
/// fails with, or a failure to write the line.
pub(crate) fn asked<W: Write, T>(
    from: Input,
    said_on: &mut W,
    unasked: Unasked<T>,
    ask: impl FnOnce(&mut W) -> Result<T>,
) -> Result<T> {
    match (from, unasked) {
        (Input::Terminal, _) => ask(said_on),
        (Input::NotTerminal, Unasked::Take(answer, said)) => {
            writeln!(said_on, "{said}").context("Failed to write what was taken")?;
            Ok(answer)
        }
        (Input::NotTerminal, Unasked::Refuse(sentence)) => Err(anyhow!(sentence)),
    }
}

/// A yes/no question, asked on stdout of stdin's terminal, Enter being
/// `enter`; with no terminal, `unasked`.
///
/// # Errors
///
/// As [`asked`].
pub(crate) fn yes_no(question: &str, enter: bool, unasked: Unasked<bool>) -> Result<bool> {
    on_stdin(|mut input| {
        yes_no_from(
            Input::stdin(),
            &mut input,
            &mut io::stdout(),
            question,
            enter,
            unasked,
        )
    })
}

/// [`yes_no`], answered from `input` when `from` is a terminal: read by
/// [`confirm_from`].
///
/// # Errors
///
/// As [`asked`].
pub(crate) fn yes_no_from(
    from: Input,
    input: &mut impl BufRead,
    asked_on: &mut impl Write,
    question: &str,
    enter: bool,
    unasked: Unasked<bool>,
) -> Result<bool> {
    asked(from, asked_on, unasked, |asked_on| {
        confirm_from(input, asked_on, question, enter)
    })
}

/// A positive number, asked on stdout of stdin's terminal; with no
/// terminal, `unasked`. A taken answer is also the one Enter takes, shown in
/// brackets.
///
/// # Errors
///
/// As [`asked`], and at the end of a terminal's input with no answer to
/// take.
pub(crate) fn number(question: &str, unasked: Unasked<f64>) -> Result<f64> {
    on_stdin(|mut input| {
        number_from(
            Input::stdin(),
            &mut input,
            &mut io::stdout(),
            question,
            unasked,
        )
    })
}

/// [`number`], answered from `input` when `from` is a terminal.
///
/// # Errors
///
/// As [`number`].
pub(crate) fn number_from(
    from: Input,
    input: &mut impl BufRead,
    asked_on: &mut impl Write,
    question: &str,
    unasked: Unasked<f64>,
) -> Result<f64> {
    let stated = match &unasked {
        Unasked::Take(answer, _) => Some(*answer),
        Unasked::Refuse(_) => None,
    };
    asked(from, asked_on, unasked, |asked_on| {
        number_typed(input, asked_on, question, stated)
    })
}

/// A positive number typed in answer to `question`, asked again until it is
/// one. Enter takes `stated`, and so does the end of input (Ctrl+D); with
/// nothing stated, the end of input is an error, since nothing more will be
/// typed.
fn number_typed(
    input: &mut impl BufRead,
    asked_on: &mut impl Write,
    question: &str,
    stated: Option<f64>,
) -> Result<f64> {
    loop {
        match stated {
            Some(answer) => writeln!(asked_on, "{question} [{answer:.1}]: "),
            None => writeln!(asked_on, "{question}: "),
        }
        .context("Failed to write the question")?;

        let mut line = String::new();
        let read = input
            .read_line(&mut line)
            .context("Failed to read user input")?;
        let typed = line.trim();
        if read == 0 || typed.is_empty() {
            if let Some(answer) = stated {
                return Ok(answer);
            }
            if read == 0 {
                bail!("Nothing was typed in answer to \"{question}\".");
            }
        }
        match typed.parse::<f64>() {
            Ok(answer) if answer > 0.0 => return Ok(answer),
            Ok(_) => eprintln!("Please enter a positive number."),
            Err(_) => eprintln!("Please enter a valid number."),
        }
    }
}

/// A yes/no question asked on stdout of a terminal a caller has already
/// found, by [`Input::stdin`], Enter being yes.
///
/// [`yes_no`] is the question with the rule in it. This one is for an offer
/// that decides on its own what happens with nobody to ask, before it asks
/// (`config::engine`'s install offer).
///
/// # Errors
///
/// Returns an error if the question cannot be written or the answer read.
pub(crate) fn prompt_confirmation_default_yes(prompt: &str) -> Result<bool> {
    on_stdin(|mut input| confirm_from(&mut input, &mut io::stdout(), prompt, true))
}

/// Ask `prompt` on `asked_on` until a line read from `input` answers it: `y`
/// or `yes` is yes, `n` or `no` is no, in any case and with any space around
/// them, and an empty line is `default`. The end of `input` (Ctrl+D) is no
/// whatever the default, since nobody is there to have agreed.
///
/// How a terminal's yes/no answer is read. It is asked of a terminal only:
/// [`yes_no_from`] puts it there, and a caller that asks it directly, on
/// stderr because its stdout is its result, has found one first.
///
/// # Errors
///
/// Returns an error if the question cannot be written or the answer read.
pub(crate) fn confirm_from(
    input: &mut impl BufRead,
    asked_on: &mut impl Write,
    prompt: &str,
    default: bool,
) -> Result<bool> {
    let hint = if default { "(Y/n)" } else { "(y/N)" };
    loop {
        writeln!(asked_on, "{prompt} {hint}: ").context("Failed to write the question")?;

        let mut line = String::new();
        let read = input
            .read_line(&mut line)
            .context("Failed to read user input")?;
        if read == 0 {
            return Ok(false);
        }
        match line.trim().to_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            "" => return Ok(default),
            _ => eprintln!("Please enter 'y' for yes or 'n' for no."),
        }
    }
}

/// Run `read` on what the terminal types: stdin, or in a test what
/// `TYPED` holds.
fn on_stdin<T>(read: impl FnOnce(&mut dyn BufRead) -> Result<T>) -> Result<T> {
    #[cfg(test)]
    if let Ok(typed) = TYPED.try_with(|typed| *typed) {
        return read(&mut typed.as_bytes());
    }
    read(&mut io::stdin().lock())
}

#[cfg(test)]
tokio::task_local! {
    /// In a test, what is typed at the terminal: a task that sets it has
    /// one ([`Input::stdin`]), and each question it asks reads its answer
    /// from this in place of stdin. The test binary's stdin is whatever the
    /// tests were run from, and one for every test in it.
    pub(crate) static TYPED: &'static str;
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
