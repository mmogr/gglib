#[cfg(test)]
mod tests {
    use super::{SecondLaunch, SecondLaunchPayload, second_launch};
    use crate::autostart::LOGIN_ITEM_FLAG;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn the_arguments_are_forwarded_in_order_without_the_program() {
        let launch = second_launch(
            strings(&[
                "/Applications/gglib.app/Contents/MacOS/gglib-gui",
                "gglib://open",
                "--x",
                "b",
            ]),
            "/Users/someone".to_owned(),
        );

        assert_eq!(
            launch,
            SecondLaunch {
                show_main_window: true,
                payload: SecondLaunchPayload {
                    args: strings(&["gglib://open", "--x", "b"]),
                    cwd: "/Users/someone".to_owned(),
                },
            }
        );
    }

    #[test]
    fn the_login_item_flag_does_not_keep_the_window_hidden() {
        let launch = second_launch(
            strings(&["/usr/bin/gglib-gui", "a", LOGIN_ITEM_FLAG, "b"]),
            "/home/someone".to_owned(),
        );

        assert_eq!(
            launch,
            SecondLaunch {
                show_main_window: true,
                payload: SecondLaunchPayload {
                    args: strings(&["a", "b"]),
                    cwd: "/home/someone".to_owned(),
                },
            }
        );
    }

    #[test]
    fn an_empty_argument_list_still_shows_the_window() {
        assert_eq!(
            second_launch(Vec::new(), String::new()),
            SecondLaunch {
                show_main_window: true,
                payload: SecondLaunchPayload {
                    args: Vec::new(),
                    cwd: String::new(),
                },
            }
        );
    }

    #[test]
    fn the_payload_reaches_the_page_as_its_args_and_cwd() {
        let payload = second_launch(strings(&["gglib-gui", "a"]), "/x".to_owned()).payload;

        assert_eq!(
            serde_json::to_value(&payload).unwrap(),
            serde_json::json!({ "args": ["a"], "cwd": "/x" })
        );
    }
}
