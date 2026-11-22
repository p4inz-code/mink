//! Unit tests for `mink setup`: parsing, answers, the plan and the interactive
//! flow, all against a scripted console and a fake script runner, so nothing
//! touches the machine.

use super::*;
use std::cell::RefCell;
use std::collections::VecDeque;

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// A console fed from a list of answers, recording everything printed.
struct Scripted {
    answers: VecDeque<String>,
    output: String,
    interactive: bool,
}

impl Scripted {
    fn new(answers: &[&str]) -> Self {
        Self {
            answers: answers.iter().map(|s| s.to_string()).collect(),
            output: String::new(),
            interactive: true,
        }
    }
}

impl Console for Scripted {
    fn print(&mut self, text: &str) {
        self.output.push_str(text);
    }

    fn read_line(&mut self) -> Option<String> {
        self.answers.pop_front()
    }

    fn is_interactive(&self) -> bool {
        self.interactive
    }
}

/// A runner that records each call and answers `-Status` from a fixed text.
struct Fake {
    calls: RefCell<Vec<(String, Vec<String>)>>,
    status: String,
    register_code: i32,
}

impl Fake {
    fn new(status: &str) -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            status: status.to_string(),
            register_code: 0,
        }
    }

    fn calls_to(&self, script: &str) -> Vec<Vec<String>> {
        self.calls
            .borrow()
            .iter()
            .filter(|(name, _)| name.ends_with(script))
            .map(|(_, args)| args.clone())
            .collect()
    }
}

impl ScriptRunner for Fake {
    fn run(&self, script: &Path, args: &[String]) -> Result<ScriptOutput, String> {
        self.calls
            .borrow_mut()
            .push((script.display().to_string(), args.to_vec()));
        if args.iter().any(|a| a == "-Status") {
            return Ok(ScriptOutput {
                code: 0,
                text: self.status.clone(),
            });
        }
        Ok(ScriptOutput {
            code: self.register_code,
            text: "script output\n".to_string(),
        })
    }
}

const NOT_SET_UP: &str = "registered=false\nowner=\nicon=\neditor=\ndefault_verb=\nverbs=\nnew_menu=false\npreview=false\ndefault_apps=false\nmink_exe=\n";
const SET_UP: &str = "registered=true\nowner=MINK.SourceFile\nicon=\"C:\\m\\mink-file.ico\",0\neditor=\ndefault_verb=edit\nverbs=run,check,build,edit\nnew_menu=true\npreview=true\ndefault_apps=true\nmink_exe=C:\\m\\bin\\mink.exe\n";
const OTHER_OWNER: &str = "registered=false\nowner=Contoso.SourceFile\nicon=\neditor=\ndefault_verb=\nverbs=\nnew_menu=false\npreview=false\ndefault_apps=false\nmink_exe=\n";

fn environment() -> Environment {
    Environment {
        mink_exe: PathBuf::from("C:\\m\\bin\\mink.exe"),
        script: PathBuf::from("C:\\m\\windows\\register-mink-filetype.ps1"),
        path_script: Some(PathBuf::from("C:\\m\\windows\\mink-path.ps1")),
        editors: vec![Editor {
            name: "Visual Studio Code".to_string(),
            path: PathBuf::from("C:\\vs\\Code.exe"),
        }],
        on_path: false,
    }
}

// ----------------------------------------------------------------- arguments

#[test]
fn arguments_parse_into_modes_and_switches() {
    assert_eq!(parse_args(&[]).unwrap().mode, Mode::Ask);
    assert_eq!(parse_args(&args(&["--yes"])).unwrap().mode, Mode::Yes);
    assert_eq!(parse_args(&args(&["-y"])).unwrap().mode, Mode::Yes);
    assert_eq!(parse_args(&args(&["--auto"])).unwrap().mode, Mode::Auto);
    assert_eq!(parse_args(&args(&["--status"])).unwrap().mode, Mode::Status);
    assert_eq!(parse_args(&args(&["--remove"])).unwrap().mode, Mode::Remove);
    let all = parse_args(&args(&[
        "--yes", "--force", "--quiet", "--no-context-menu", "--no-new-menu", "--no-preview",
        "--no-default-apps", "--no-file-type", "--no-path",
    ]))
    .unwrap();
    assert!(all.force && all.quiet && all.no_context_menu && all.no_new_menu);
    assert!(all.no_preview && all.no_default_apps && all.no_file_type);
    assert_eq!(all.path, Some(false));
    assert_eq!(parse_args(&args(&["--path"])).unwrap().path, Some(true));
}

#[test]
fn the_editor_argument_takes_a_path_or_a_keyword() {
    let editor = |v: &str| parse_args(&args(&["--editor", v])).unwrap().editor;
    assert_eq!(editor("notepad"), EditorArg::Notepad);
    assert_eq!(editor("C:\\e\\code.exe"), EditorArg::Path(PathBuf::from("C:\\e\\code.exe")));
    assert!(parse_args(&args(&["--editor"])).is_err());
    let refused = parse_args(&args(&["--editor", "windows"])).unwrap_err();
    assert!(refused.contains("--no-context-menu"), "{refused}");
}

#[test]
fn unknown_options_are_reported_by_name() {
    assert_eq!(
        parse_args(&args(&["--bogus"])).unwrap_err(),
        "unknown option '--bogus' for 'setup'"
    );
}

// ------------------------------------------------------------------- answers

#[test]
fn selections_accept_numbers_ranges_and_words() {
    let defaults = [1, 2];
    assert_eq!(parse_selection("", 5, &defaults).unwrap(), vec![1, 2]);
    assert_eq!(parse_selection("1 3", 5, &defaults).unwrap(), vec![1, 3]);
    assert_eq!(parse_selection("3,1", 5, &defaults).unwrap(), vec![1, 3], "sorted");
    assert_eq!(parse_selection("1, 3 ,5", 5, &defaults).unwrap(), vec![1, 3, 5]);
    assert_eq!(parse_selection("2-4", 5, &defaults).unwrap(), vec![2, 3, 4]);
    assert_eq!(parse_selection("1 1 2", 5, &defaults).unwrap(), vec![1, 2], "no duplicates");
    assert_eq!(parse_selection("all", 3, &defaults).unwrap(), vec![1, 2, 3]);
    assert_eq!(parse_selection("ALL", 3, &defaults).unwrap(), vec![1, 2, 3]);
    assert_eq!(parse_selection("none", 3, &defaults).unwrap(), Vec::<usize>::new());
}

#[test]
fn bad_selections_say_what_is_wrong() {
    let err = |t: &str| parse_selection(t, 5, &[1]).unwrap_err();
    assert!(err("6").contains("outside the list"));
    assert!(err("0").contains("outside the list"));
    assert!(err("4-2").contains("outside the list"));
    assert!(err("x").contains("not a number"));
    assert!(err("1 two").contains("'two'"));
}

#[test]
fn yes_no_answers() {
    assert_eq!(parse_yes_no("y", false), Some(true));
    assert_eq!(parse_yes_no("YES", false), Some(true));
    assert_eq!(parse_yes_no("n", true), Some(false));
    assert_eq!(parse_yes_no("No", true), Some(false));
    assert_eq!(parse_yes_no("", true), Some(true));
    assert_eq!(parse_yes_no("  ", false), Some(false));
    assert_eq!(parse_yes_no("maybe", true), None);
}

// ------------------------------------------------------------------- editors

#[test]
fn only_installed_editors_are_listed() {
    let env = |name: &str| match name {
        "LOCALAPPDATA" => Some("C:\\Users\\a\\AppData\\Local".to_string()),
        "ProgramFiles" => Some("C:\\Program Files".to_string()),
        _ => None,
    };
    let present = |path: &Path| {
        let text = path.display().to_string();
        text.ends_with("Programs\\Microsoft VS Code\\Code.exe") || text.ends_with("Notepad++\\notepad++.exe")
    };
    let found = detect_editors(&env, &present);
    let names: Vec<&str> = found.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["Visual Studio Code", "Notepad++"]);
    assert!(detect_editors(&env, &|_| false).is_empty());
}

#[test]
fn an_editor_found_in_two_places_is_listed_once() {
    let env = |name: &str| match name {
        "LOCALAPPDATA" => Some("C:\\L".to_string()),
        "ProgramFiles" => Some("C:\\P".to_string()),
        _ => None,
    };
    let found = detect_editors(&env, &|_| true);
    assert_eq!(found.iter().filter(|e| e.name == "Visual Studio Code").count(), 1);
}

// ----------------------------------------------------------------- PATH text

#[test]
fn path_matching_ignores_case_quotes_and_trailing_slashes() {
    let dir = Path::new("C:\\Tools\\MINK\\bin");
    assert!(path_contains("C:\\x;c:\\tools\\mink\\bin\\;D:\\y", dir));
    assert!(path_contains("\"C:\\Tools\\MINK\\bin\"", dir));
    assert!(path_contains("C:/Tools/MINK/bin", dir));
    assert!(!path_contains("C:\\Tools\\MINK", dir));
    assert!(!path_contains("", dir));
    assert!(!path_contains(";;", dir));
}

// -------------------------------------------------------------------- status

#[test]
fn status_lines_are_read_back() {
    let status = parse_status(SET_UP);
    assert!(status.registered && status.new_menu && status.preview && status.default_apps);
    assert_eq!(status.verbs, vec!["run", "check", "build", "edit"]);
    assert_eq!(status.default_verb, "edit");
    let other = parse_status(OTHER_OWNER);
    assert!(!other.registered);
    assert_eq!(other.owner, "Contoso.SourceFile");
}

#[test]
fn status_is_described_in_plain_words() {
    let text = describe_status(&parse_status(SET_UP));
    assert!(text.contains(".mink file type : set up"));
    assert!(text.contains("opens Notepad"));
    assert!(text.contains("Run, Check, Build, Edit"));
    let other = describe_status(&parse_status(OTHER_OWNER));
    assert!(other.contains("owned by another program (Contoso.SourceFile)"));
    let none = describe_status(&parse_status(NOT_SET_UP));
    assert!(none.contains("not set up"));
}

// ---------------------------------------------------------------------- plan

#[test]
fn the_default_plan_asks_for_everything_and_names_no_editor() {
    let args = Plan::defaults().script_arguments(Path::new("C:\\m\\bin\\mink.exe"), false);
    assert_eq!(args, vec!["-MinkExe", "C:\\m\\bin\\mink.exe"]);
}

#[test]
fn each_switch_off_becomes_its_script_switch() {
    let mut plan = Plan::defaults();
    plan.context_menu = false;
    plan.new_menu = false;
    plan.preview = false;
    plan.default_apps = false;
    plan.force = true;
    plan.editor = Some(PathBuf::from("C:\\e\\code.exe"));
    let args = plan.script_arguments(Path::new("m.exe"), true);
    for expected in [
        "-NoContextMenu", "-NoNewMenu", "-NoPreview", "-NoDefaultApps", "-Force", "-Quiet", "-Editor",
        "C:\\e\\code.exe",
    ] {
        assert!(args.contains(&expected.to_string()), "{expected} in {args:?}");
    }
}

#[test]
fn the_kit_is_found_next_to_the_executable() {
    let root = std::env::temp_dir().join(format!("mink_setup_kit_{}", std::process::id()));
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::create_dir_all(root.join("windows")).unwrap();
    std::fs::write(root.join("windows").join(SCRIPT_NAME), "x").unwrap();
    let found = find_kit_file(&root.join("bin").join("mink.exe"), SCRIPT_NAME).unwrap();
    assert!(found.ends_with(SCRIPT_NAME));
    assert!(find_kit_file(&root.join("bin").join("mink.exe"), "nothing.ps1").is_none());
    let _ = std::fs::remove_dir_all(&root);
}

// ----------------------------------------------------------------- the modes

#[test]
fn status_mode_prints_and_changes_nothing() {
    let fake = Fake::new(SET_UP);
    let mut console = Scripted::new(&[]);
    let code = run_with(&SetupArgs { mode: Mode::Status, ..Default::default() }, &environment(), &mut console, &fake);
    assert_eq!(code, exit::OK);
    assert!(console.output.contains("set up"));
    assert!(console.output.contains("on your PATH    : no"));
    assert_eq!(fake.calls.borrow().len(), 1, "only -Status was run");
}

#[test]
fn remove_runs_the_uninstall_and_takes_mink_off_the_path() {
    let fake = Fake::new(SET_UP);
    let mut console = Scripted::new(&[]);
    let code = run_with(&SetupArgs { mode: Mode::Remove, ..Default::default() }, &environment(), &mut console, &fake);
    assert_eq!(code, exit::OK);
    let register = fake.calls_to(SCRIPT_NAME);
    assert_eq!(register, vec![vec!["-Uninstall".to_string(), "-Quiet".to_string()]]);
    let path = fake.calls_to(PATH_SCRIPT_NAME);
    assert_eq!(path[0][0], "-Remove");
    assert!(console.output.contains("Removed"));
}

#[test]
fn a_failed_removal_is_reported_with_a_failure_code() {
    let mut fake = Fake::new(SET_UP);
    fake.register_code = 1;
    let mut console = Scripted::new(&[]);
    let code = run_with(&SetupArgs { mode: Mode::Remove, ..Default::default() }, &environment(), &mut console, &fake);
    assert_eq!(code, exit::FAILED);
    assert!(console.output.contains("removal failed"));
}

#[test]
fn auto_applies_defaults_to_a_new_setup_without_a_word() {
    let fake = Fake::new(NOT_SET_UP);
    let mut console = Scripted::new(&[]);
    let code = run_with(&SetupArgs { mode: Mode::Auto, ..Default::default() }, &environment(), &mut console, &fake);
    assert_eq!(code, exit::OK);
    assert_eq!(console.output, "", "auto prints nothing");
    let register = fake.calls_to(SCRIPT_NAME);
    assert_eq!(register.len(), 2, "status, then register");
    assert!(register[1].contains(&"-Quiet".to_string()));
    assert!(!register[1].contains(&"-Force".to_string()), "auto never takes .mink over");
    assert!(fake.calls_to(PATH_SCRIPT_NAME).is_empty(), "auto never changes PATH");
}

#[test]
fn auto_refreshes_an_existing_setup_and_keeps_its_choices() {
    let fake = Fake::new(SET_UP);
    let mut console = Scripted::new(&[]);
    let code = run_with(&SetupArgs { mode: Mode::Auto, ..Default::default() }, &environment(), &mut console, &fake);
    assert_eq!(code, exit::OK);
    let register = fake.calls_to(SCRIPT_NAME);
    assert_eq!(register[1][0], "-Refresh");
    assert!(!register[1].contains(&"-NoNewMenu".to_string()));
}

#[test]
fn auto_leaves_another_programs_association_alone() {
    let fake = Fake::new(OTHER_OWNER);
    let mut console = Scripted::new(&[]);
    let code = run_with(&SetupArgs { mode: Mode::Auto, ..Default::default() }, &environment(), &mut console, &fake);
    assert_eq!(code, exit::OK);
    assert_eq!(fake.calls_to(SCRIPT_NAME).len(), 1, "only the status was read");
}

#[test]
fn yes_binds_the_first_detected_editor_and_adds_to_path() {
    let fake = Fake::new(NOT_SET_UP);
    let mut console = Scripted::new(&[]);
    let code = run_with(&SetupArgs { mode: Mode::Yes, ..Default::default() }, &environment(), &mut console, &fake);
    assert_eq!(code, exit::OK);
    let register = &fake.calls_to(SCRIPT_NAME)[1];
    assert!(register.windows(2).any(|w| w == ["-Editor", "C:\\vs\\Code.exe"]));
    let path = fake.calls_to(PATH_SCRIPT_NAME);
    assert_eq!(path[0][0], "-Add");
    assert!(console.output.contains("Done."));
}

#[test]
fn yes_respects_no_path_and_an_explicit_editor() {
    let fake = Fake::new(NOT_SET_UP);
    let mut console = Scripted::new(&[]);
    let parsed = SetupArgs {
        mode: Mode::Yes,
        path: Some(false),
        editor: EditorArg::Notepad,
        ..Default::default()
    };
    run_with(&parsed, &environment(), &mut console, &fake);
    assert!(fake.calls_to(PATH_SCRIPT_NAME).is_empty());
    assert!(!fake.calls_to(SCRIPT_NAME)[1].contains(&"-Editor".to_string()));
}

#[test]
fn yes_does_not_take_over_another_program_without_force() {
    let fake = Fake::new(OTHER_OWNER);
    let mut console = Scripted::new(&[]);
    let code = run_with(&SetupArgs { mode: Mode::Yes, ..Default::default() }, &environment(), &mut console, &fake);
    assert_eq!(code, exit::OK);
    assert!(console.output.contains("already belongs to another program (Contoso.SourceFile)"));
    assert_eq!(fake.calls_to(SCRIPT_NAME).len(), 1);
    let fake = Fake::new(OTHER_OWNER);
    let mut console = Scripted::new(&[]);
    run_with(&SetupArgs { mode: Mode::Yes, force: true, ..Default::default() }, &environment(), &mut console, &fake);
    assert!(fake.calls_to(SCRIPT_NAME)[1].contains(&"-Force".to_string()));
}

#[test]
fn a_script_failure_gives_the_failure_code() {
    let mut fake = Fake::new(NOT_SET_UP);
    fake.register_code = 1;
    let mut console = Scripted::new(&[]);
    let code = run_with(&SetupArgs { mode: Mode::Yes, ..Default::default() }, &environment(), &mut console, &fake);
    assert_eq!(code, exit::FAILED);
    assert!(console.output.contains("could not be set up"));
}

// ----------------------------------------------------------------- questions

#[test]
fn asking_without_a_terminal_explains_what_to_do() {
    let fake = Fake::new(NOT_SET_UP);
    let mut console = Scripted::new(&[]);
    console.interactive = false;
    let code = run_with(&SetupArgs::default(), &environment(), &mut console, &fake);
    assert_eq!(code, exit::USAGE);
    assert!(console.output.contains("mink setup --yes"));
    assert!(fake.calls.borrow().is_empty(), "nothing was run");
}

#[test]
fn the_question_flow_with_all_defaults() {
    let fake = Fake::new(NOT_SET_UP);
    // Enter keeps the marked items, Enter picks the first editor, Enter adds PATH, Enter applies.
    let mut console = Scripted::new(&["", "", "", ""]);
    let code = run_with(&SetupArgs::default(), &environment(), &mut console, &fake);
    assert_eq!(code, exit::OK, "{}", console.output);
    assert!(console.output.contains("[x] 1  File type and icon"));
    assert!(console.output.contains("1  Visual Studio Code"));
    assert!(console.output.contains("2  Notepad"));
    assert!(console.output.contains("Add MINK to your PATH"));
    let register = &fake.calls_to(SCRIPT_NAME)[1];
    assert!(register.windows(2).any(|w| w == ["-Editor", "C:\\vs\\Code.exe"]));
    assert_eq!(fake.calls_to(PATH_SCRIPT_NAME)[0][0], "-Add");
    assert!(console.output.contains("Open a new terminal"));
}

#[test]
fn a_custom_selection_turns_parts_off() {
    let fake = Fake::new(NOT_SET_UP);
    // Only the file type and the New menu; Notepad; no PATH; apply.
    let mut console = Scripted::new(&["1 3", "2", "n", "y"]);
    let code = run_with(&SetupArgs::default(), &environment(), &mut console, &fake);
    assert_eq!(code, exit::OK, "{}", console.output);
    let register = &fake.calls_to(SCRIPT_NAME)[1];
    for off in ["-NoContextMenu", "-NoPreview", "-NoDefaultApps"] {
        assert!(register.contains(&off.to_string()), "{off} in {register:?}");
    }
    assert!(!register.contains(&"-NoNewMenu".to_string()));
    assert!(!register.contains(&"-Editor".to_string()), "Notepad binds no editor");
    assert!(fake.calls_to(PATH_SCRIPT_NAME).is_empty());
}

#[test]
fn bad_answers_are_asked_again() {
    let fake = Fake::new(NOT_SET_UP);
    let mut console = Scripted::new(&["9", "all", "", "maybe", "n", "y"]);
    let code = run_with(&SetupArgs::default(), &environment(), &mut console, &fake);
    assert_eq!(code, exit::OK, "{}", console.output);
    assert!(console.output.contains("outside the list"));
    assert!(console.output.contains("Please answer y or n"));
}

#[test]
fn declining_the_confirmation_changes_nothing() {
    let fake = Fake::new(NOT_SET_UP);
    let mut console = Scripted::new(&["", "", "", "n"]);
    let code = run_with(&SetupArgs::default(), &environment(), &mut console, &fake);
    assert_eq!(code, exit::OK);
    assert!(console.output.contains("Nothing was changed."));
    assert_eq!(fake.calls_to(SCRIPT_NAME).len(), 1, "only -Status ran");
}

#[test]
fn choosing_none_does_nothing() {
    let fake = Fake::new(NOT_SET_UP);
    let mut console = Scripted::new(&["none", "n"]);
    let code = run_with(&SetupArgs::default(), &environment(), &mut console, &fake);
    assert_eq!(code, exit::OK);
    assert!(console.output.contains("Nothing to do."));
}

#[test]
fn another_programs_association_is_only_taken_after_a_yes() {
    let fake = Fake::new(OTHER_OWNER);
    let mut console = Scripted::new(&["", "", "n", "y", "y"]);
    let code = run_with(&SetupArgs::default(), &environment(), &mut console, &fake);
    assert_eq!(code, exit::OK, "{}", console.output);
    assert!(console.output.contains("Contoso.SourceFile"));
    assert!(fake.calls_to(SCRIPT_NAME)[1].contains(&"-Force".to_string()));

    let fake = Fake::new(OTHER_OWNER);
    let mut console = Scripted::new(&["", "", "n", "n"]);
    run_with(&SetupArgs::default(), &environment(), &mut console, &fake);
    assert!(console.output.contains("Leaving .mink with the other program."));
    assert_eq!(fake.calls_to(SCRIPT_NAME).len(), 1, "nothing was registered");
}

#[test]
fn running_out_of_input_stops_without_changing_anything() {
    let fake = Fake::new(NOT_SET_UP);
    let mut console = Scripted::new(&[""]);
    let code = run_with(&SetupArgs::default(), &environment(), &mut console, &fake);
    assert_eq!(code, exit::USAGE);
    assert!(console.output.contains("nothing was changed"));
    assert_eq!(fake.calls_to(SCRIPT_NAME).len(), 1);
}

#[test]
fn flags_given_on_the_command_line_skip_their_questions() {
    let fake = Fake::new(NOT_SET_UP);
    // The editor and PATH are decided by flags, so only the list and the confirmation are asked.
    let parsed = SetupArgs {
        editor: EditorArg::Notepad,
        path: Some(false),
        ..Default::default()
    };
    let mut console = Scripted::new(&["", "y"]);
    let code = run_with(&parsed, &environment(), &mut console, &fake);
    assert_eq!(code, exit::OK, "{}", console.output);
    assert!(!console.output.contains("double-clicking a .mink file open"));
    assert!(!console.output.contains("Add MINK to your PATH"));
}
