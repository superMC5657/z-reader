// 在 Windows Release 构建下隐藏额外的控制台窗口，请勿删除！！
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    zreader_lib::run()
}
