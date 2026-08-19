//! SSE 流式解码。
//!
//! 这里测的是 `SseDecoder` 这个公开接口：喂字节分片，吐翻译事件。
//! 它从 HTTP 客户端里独立出来，就是为了能这样脱离网络测。

use seltrans_core::llm::{Event, SseDecoder};

/// 把解码结果里的正文拼起来，方便断言
fn text_of(events: &[Event]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Delta(s) => Some(s.as_str()),
            _ => None,
        })
        .collect()
}

/// 服务端不保证按字符边界切分片。一个汉字三字节，要是在中间切开就按字节转成字符串，
/// 会得到一串替换字符 —— 表现就是译文里冒出「」。必须缓冲到换行再解。
#[test]
fn 汉字被拦腰切断也不该乱码() {
    let line = "data: {\"choices\":[{\"delta\":{\"content\":\"你好\"}}]}\n".as_bytes();

    // 切在「你」的第一个字节之后，即三字节序列的中间
    let cut = line.iter().position(|&b| b == 0xe4).unwrap() + 1;
    let (head, tail) = line.split_at(cut);

    let mut d = SseDecoder::new(false);
    let mut events = d.push(head);
    events.extend(d.push(tail));

    assert_eq!(text_of(&events), "你好");
}

/// 一行还没收完（没等到换行符）就不能急着解，否则半截 JSON 会被丢掉
#[test]
fn 行没收完时先不吐事件() {
    let mut d = SseDecoder::new(false);
    assert!(d.push(b"data: {\"choices\":[{\"delta\":{\"cont").is_empty());
    let events = d.push(b"ent\":\"ok\"}}]}\n");
    assert_eq!(text_of(&events), "ok");
}

#[test]
fn openai_取正文而非思考过程() {
    let mut d = SseDecoder::new(false);
    // 有些服务把推理过程放 reasoning_content，只有 content 才是译文
    let events = d.push(
        b"data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"\xe6\x83\xb3\",\"content\":\"\xe8\xaf\x91\"}}]}\n",
    );
    assert_eq!(text_of(&events), "译");
}

#[test]
fn anthropic_只认_text_delta() {
    let mut d = SseDecoder::new(true);
    let events = d.push(
        b"event: message_start\n\
          data: {\"type\":\"message_start\"}\n\
          data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\
          data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\"}}\n",
    );
    assert_eq!(text_of(&events), "hi");
}

/// SSE 的心跳注释行和事件分隔空行都不该产生事件
#[test]
fn 忽略心跳与空行() {
    let mut d = SseDecoder::new(false);
    let events =
        d.push(b": keep-alive\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"x\"}}]}\n\n");
    assert_eq!(events.len(), 1);
    assert_eq!(text_of(&events), "x");
}

#[test]
fn done_之后不再吐事件() {
    let mut d = SseDecoder::new(false);
    let events = d.push(b"data: {\"choices\":[{\"delta\":{\"content\":\"a\"}}]}\ndata: [DONE]\n");
    assert_eq!(text_of(&events), "a");
    assert!(matches!(events.last(), Some(Event::Done)));
    assert!(d.is_finished());

    // 服务端在 [DONE] 后又发了东西也不该再吐
    assert!(
        d.push(b"data: {\"choices\":[{\"delta\":{\"content\":\"b\"}}]}\n")
            .is_empty()
    );
}

#[test]
fn error_事件带出服务端的说明并终止() {
    let mut d = SseDecoder::new(false);
    let events = d.push(b"data: {\"error\":{\"message\":\"insufficient balance\"}}\n");

    match events.as_slice() {
        [Event::Error(msg)] => assert_eq!(msg, "insufficient balance"),
        other => panic!("期望单个 Error 事件，实际：{other:?}"),
    }
    assert!(d.is_finished());
}
