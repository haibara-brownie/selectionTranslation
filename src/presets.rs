//! 内置预设：供应商目录 + 提示词模板。
//!
//! 供应商预设**只提供 base_url**，不写死模型名 —— 各家模型迭代太快（`deepseek-chat`
//! 已在 2026-07-24 停用、`moonshot-v1` 系列 2026-08-31 退役），模型列表一律在配置
//! 界面里点「拉取模型」实时从 `/v1/models` 取。

pub struct ProviderPreset {
    pub id: &'static str,
    pub name: &'static str,
    /// "openai"（走 /chat/completions）或 "anthropic"（走 /v1/messages）
    pub kind: &'static str,
    pub base_url: &'static str,
    /// 申请 API key 的页面
    pub keys_url: &'static str,
    /// 界面上的补充说明
    pub hint: &'static str,
}

pub const PROVIDER_PRESETS: &[ProviderPreset] = &[
    ProviderPreset {
        id: "deepseek",
        name: "DeepSeek",
        kind: "openai",
        base_url: "https://api.deepseek.com/v1",
        keys_url: "https://platform.deepseek.com/api_keys",
        hint: "deepseek-chat / deepseek-reasoner 已于 2026-07-24 停用，请用 v4 系列。\
               deepseek-v4-flash 延迟低、适合划词；deepseek-v4-pro 是旗舰推理档。",
    },
    ProviderPreset {
        id: "zhipu",
        name: "智谱 GLM",
        kind: "openai",
        base_url: "https://open.bigmodel.cn/api/paas/v4",
        keys_url: "https://open.bigmodel.cn/usercenter/apikeys",
        hint: "glm-4.7-flash 免费，很适合当划词翻译的日常档；glm-5.3 / glm-5.2 是旗舰档。",
    },
    ProviderPreset {
        id: "moonshot",
        name: "Kimi（Moonshot）",
        kind: "openai",
        base_url: "https://api.moonshot.ai/v1",
        keys_url: "https://platform.kimi.com",
        hint: "kimi-k3 强制开启思考、按全量推理轨迹计费，划词翻译成本偏高；\
               日常建议用 kimi-k2.6。moonshot-v1 系列 2026-08-31 退役。\
               国内直连可把域名换成 api.moonshot.cn。",
    },
    ProviderPreset {
        id: "siliconflow",
        name: "硅基流动",
        kind: "openai",
        base_url: "https://api.siliconflow.cn/v1",
        keys_url: "https://cloud.siliconflow.cn/account/ak",
        hint: "聚合平台，模型很多，建议拉取列表后挑一个小参数量的快模型。",
    },
    ProviderPreset {
        id: "dashscope",
        name: "阿里百炼（通义千问）",
        kind: "openai",
        base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1",
        keys_url: "https://bailian.console.aliyun.com/?tab=model#/api-key",
        hint: "新版控制台给的是带 WorkspaceId 的地址，形如 \
               https://{WorkspaceId}.cn-beijing.maas.aliyuncs.com/compatible-mode/v1，\
               若这里的经典域名调不通请换成控制台里的那个。\
               注意 qwen3.8-max 默认开启思考模式。",
    },
    ProviderPreset {
        id: "openrouter",
        name: "OpenRouter",
        kind: "openai",
        base_url: "https://openrouter.ai/api/v1",
        keys_url: "https://openrouter.ai/keys",
        hint: "聚合各家模型，模型名形如 provider/model。",
    },
    ProviderPreset {
        id: "openai",
        name: "OpenAI",
        kind: "openai",
        base_url: "https://api.openai.com/v1",
        keys_url: "https://platform.openai.com/api-keys",
        hint: "gpt-5.6-luna 便宜且快，适合划词；gpt-5.6（Sol）是旗舰档。",
    },
    ProviderPreset {
        id: "anthropic",
        name: "Anthropic Claude",
        kind: "anthropic",
        base_url: "https://api.anthropic.com",
        keys_url: "https://console.anthropic.com/settings/keys",
        hint: "走 Messages API。注意当前模型不接受 temperature 参数（会 400），\
               本程序已针对性地不发送该参数。claude-haiku-4-5 便宜快，\
               claude-opus-5 / claude-sonnet-5 质量更高。",
    },
    ProviderPreset {
        id: "ollama",
        name: "Ollama（本地）",
        kind: "openai",
        base_url: "http://localhost:11434/v1",
        keys_url: "",
        hint: "本地模型，API key 留空即可。需先 ollama serve 并 pull 过模型。",
    },
    ProviderPreset {
        id: "custom",
        name: "自定义（OpenAI 兼容）",
        kind: "openai",
        base_url: "",
        keys_url: "",
        hint: "任何提供 OpenAI 兼容 /chat/completions 接口的服务都可以填在这里。",
    },
];

pub fn preset_by_id(id: &str) -> Option<&'static ProviderPreset> {
    PROVIDER_PRESETS.iter().find(|p| p.id == id)
}

/// 目标语言候选。`(写进提示词的值, 界面上显示的标签)`
///
/// 存的是**模型看得懂的语言名**而不是 ISO 代码 —— 它会直接替换掉提示词里的
/// `{target_lang}`，写 "简体中文" 比写 "zh-Hans" 的效果稳定得多。
/// 这里列的是各家主流大模型翻译质量都比较可靠的语种。
pub const TARGET_LANGS: &[(&str, &str)] = &[
    ("简体中文", "简体中文"),
    ("繁體中文", "繁体中文 繁體中文"),
    ("English", "英语 English"),
    ("日本語", "日语 日本語"),
    ("한국어", "韩语 한국어"),
    ("Français", "法语 Français"),
    ("Deutsch", "德语 Deutsch"),
    ("Español", "西班牙语 Español"),
    ("Português", "葡萄牙语 Português"),
    ("Italiano", "意大利语 Italiano"),
    ("Русский", "俄语 Русский"),
    ("Українська", "乌克兰语 Українська"),
    ("Nederlands", "荷兰语 Nederlands"),
    ("Polski", "波兰语 Polski"),
    ("Svenska", "瑞典语 Svenska"),
    ("Türkçe", "土耳其语 Türkçe"),
    ("العربية", "阿拉伯语 العربية"),
    ("हिन्दी", "印地语 हिन्दी"),
    ("ภาษาไทย", "泰语 ภาษาไทย"),
    ("Tiếng Việt", "越南语 Tiếng Việt"),
    ("Bahasa Indonesia", "印尼语 Bahasa Indonesia"),
];

/// 内置提示词。`{target_lang}` 会在调用前替换成配置里的目标语言。
pub struct PromptPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub icon: &'static str,
    pub system: &'static str,
}

pub const PROMPT_PRESETS: &[PromptPreset] = &[
    PromptPreset {
        id: "general",
        name: "通用翻译",
        icon: "🌐",
        system: "你是一个专业翻译引擎。把用户提供的文本翻译成{target_lang}。

要求：
- 只输出译文本身，不要输出任何解释、前言、引号，也不要加「译文：」之类的标签。
- 忠实于原意，同时符合{target_lang}的表达习惯，不要逐字硬译。
- 保留原文的段落结构和换行。
- 如果原文已经是{target_lang}，就把它改写得更通顺自然。
- 人名、品牌名、专有名词如果没有通用译名就保留原文。",
    },
    PromptPreset {
        id: "github",
        name: "GitHub / 技术文档",
        icon: "💻",
        system: "你是一名面向程序员的技术翻译。把用户提供的技术文本（README、issue、\
commit message、API 文档、报错说明等）翻译成{target_lang}。

要求：
- 只输出译文，不要任何解释或前言。
- 以下内容一律保持原样不译：代码片段、变量名 / 函数名 / 类名、命令行、文件路径、URL、\
环境变量、错误码、包名。
- 完整保留 Markdown 结构：标题层级、列表、表格、行内代码 `...`、代码块 ``` 都原样保留。
- 已经成为行业惯例的英文词保留英文（如 commit、pull request、build、CI、fork），不要生硬直译。
- 技术术语首次出现时可以用「中文（English）」的形式给出对照。",
    },
    PromptPreset {
        id: "paper",
        name: "科学杂志 / 论文",
        icon: "🔬",
        system: "你是一名学术论文译者。把用户提供的学术文本翻译成{target_lang}。

要求：
- 只输出译文，不要任何解释或前言。
- 使用严谨的学术书面语，避免口语化表达和网络用语。
- 专业术语首次出现时用「中文译名（English Term）」的形式给出对照，之后统一使用中文译名。
- 完整保留：LaTeX 公式、数学符号、计量单位、化学式、基因 / 物种名、\
文献引用标记（如 [12]、(Smith et al., 2020)）。
- 保持长句的逻辑关系（因果、转折、递进）清晰，必要时拆成多个短句。",
    },
    PromptPreset {
        id: "casual",
        name: "日常口语",
        icon: "💬",
        system: "你是一名口语翻译。把用户提供的文本翻译成地道自然的{target_lang}口语。

要求：
- 只输出译文，不要任何解释或前言。
- 用日常说话的方式表达，不要书面语腔调，不要翻译腔。
- 保留原文的语气（俏皮、抱怨、惊讶、正式或随意）和表情符号。
- 俚语、梗、缩写翻译成{target_lang}里对应的自然说法；实在没有对应说法时直译，\
并在括号里补一句简短说明。",
    },
    PromptPreset {
        id: "explain",
        name: "术语解释",
        icon: "📖",
        system: "你是一名双语知识助手。用户会给你一个词、一个术语或一小段文本。

请用{target_lang}按下面的格式回答，不要输出格式以外的内容：

**译文**：<准确的{target_lang}译文>

**解释**：<2 到 4 句话，说明它是什么、用在什么场景、有什么容易混淆的地方>

如果它是某个领域的专有术语，请指出所属领域。",
    },
    PromptPreset {
        id: "code",
        name: "报错 / 代码解读",
        icon: "🐞",
        system: "你是一名资深工程师。用户会给你一段报错信息、日志或代码。

请用{target_lang}按下面的格式回答，不要输出格式以外的内容：

**含义**：<这段信息在说什么，一两句话讲清楚>

**常见原因**：<列出 2 到 3 个最可能的原因>

**建议排查**：<给出具体的下一步动作>

原文里的路径、行号、函数名、错误码保持原样不译。",
    },
    PromptPreset {
        id: "polish",
        name: "中译英润色",
        icon: "✍️",
        system: "You are a professional translator and editor. Translate the user's text \
into natural, idiomatic English, then polish it.

Rules:
- Output ONLY the final English text. No explanation, no preamble, no surrounding quotes.
- Match the register of the source: casual stays casual, formal stays formal.
- Prefer plain, direct wording over ornate phrasing. Avoid translationese.
- Leave code, identifiers, file paths, and URLs unchanged.",
    },
];
