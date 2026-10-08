# utils

<!-- module-docs:start -->

What a command asks a person on the terminal. `input.rs` is all of it: each
function prints its prompt on a line of its own and reads the answer from
stdin.

| Function | Asks for |
|----------|----------|
| [`input::prompt_string`] | A line of text |
| [`input::prompt_string_with_default`] | A line of text, or Enter for the default shown in brackets |
| [`input::prompt_confirmation`] | Yes or no, where Enter is no |
| [`input::prompt_confirmation_default_yes`] | Yes or no, where Enter is yes |
| [`input::prompt_float`] | A positive number, asked again until it is one |
| [`input::prompt_float_with_default`] | A positive number, or Enter for the default shown in brackets |

The two confirmations read an answer by one rule, [`input::confirm_from`]:
`y` or `yes` is yes and `n` or `no` is no, in any case; anything else is asked
again; and the end of input is no, whichever way Enter goes. `gglib q` asks
whether to continue chatting by the same rule, on stderr, since its stdout is
the answer it printed.

<!-- module-docs:end -->
