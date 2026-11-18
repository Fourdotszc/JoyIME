// 发布版不弹控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    joyime_app_lib::run()
}
