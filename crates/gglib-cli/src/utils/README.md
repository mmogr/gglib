# utils

<!-- module-docs:start -->

What a command asks a person on the terminal, by one rule: a question is
asked only of a terminal. `input.rs` is all of it.

Whether stdin is a terminal is asked in one place, [`input::Input::stdin`],
and passed to the rule as a value. With a terminal, the question is printed
on a line of its own and its answer read from stdin. With none (a pipe, a
file, a closed input, SSH without a terminal) the input is never read, even
when an answer is waiting in it, since one that nobody writes to and nobody
closes would be waited on for ever. The question comes to its
[`input::Unasked`] instead: `Take` an answer the command can state, printing
the line that says what was taken and why, or `Refuse` with one sentence
naming the flag that answers it ([`input::nobody_to_ask`]). Three offers
still ask stdin themselves, by the same rule, and are to be moved onto this
one: the engine install offer (`config::engine`) and `gglib up`'s two
(`up::require_tty`, `up::probe`).

| Function | Asks for |
|----------|----------|
| [`input::asked`] | Anything: the rule itself, with the question's own reading |
| [`input::yes_no`] | Yes or no, Enter being the answer named |
| [`input::number`] | A positive number, asked again until it is one; Enter takes the answer `Take` states |
| [`input::prompt_confirmation_default_yes`] | Yes or no, Enter being yes, of a terminal the caller has already found |

A yes/no answer is read by [`input::confirm_from`]: `y` or `yes` is yes and
`n` or `no` is no, in any case; anything else is asked again; and the end of
input (Ctrl+D) is no, whichever way Enter goes. `gglib q` asks whether to
continue chatting by the same rule, on stderr, since its stdout is the
answer it printed.

<!-- module-docs:end -->
