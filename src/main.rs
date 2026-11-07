//! 程序入口：初始化终端 → 运行事件循环 → 恢复终端。

use std::process::ExitCode;

use se_lab::app::App;
use se_lab::tui;

fn main() -> ExitCode {
    tui::install_panic_hook();

    let mut terminal = match tui::init() {
        Ok(terminal) => terminal,
        Err(err) => {
            eprintln!("无法初始化终端：{err}");
            return ExitCode::FAILURE;
        }
    };

    let mut app = App::new();
    let result = app.run(&mut terminal);

    if let Err(err) = tui::restore() {
        eprintln!("恢复终端失败：{err}");
    }

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("程序异常退出：{err}");
            ExitCode::FAILURE
        }
    }
}
