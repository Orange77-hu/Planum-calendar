fn main() {
    // 把 icon.ico 作为 Windows 图标资源嵌入 exe（资源管理器/任务栏显示用）
    embed_resource::compile("icon.rc", embed_resource::NONE);
}
