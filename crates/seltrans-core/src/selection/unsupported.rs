//! mac / Windows 的取词还没实现（迁移方案里的 P3）。
//!
//! 这里刻意**不做假动作** —— 与其返回空字符串让上层以为"没选中东西"，
//! 不如明确说这个平台还没做，用户至少知道该手动粘贴。

pub fn grab(_mode: &str) -> Result<String, String> {
    Err(format!(
        "{} 上的取词还没实现，请用输入模式手动粘贴要翻译的内容",
        std::env::consts::OS
    ))
}

pub fn deps_report() -> Vec<(String, bool, String)> {
    vec![(
        "取词".into(),
        false,
        format!(
            "{} 平台尚未实现取词，目前只能手动输入",
            std::env::consts::OS
        ),
    )]
}
