//! 一次性核对工具：拿真实抓回来的定价页跑一遍解析器。
//! 用法：cargo run -p qb-station --example parse-live-pricing -- <文件> <anthropic|openai>
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&args[1]).expect("读不到文件");
    let fmt = match args[2].as_str() {
        "anthropic" => qb_station::station::pricing::PricingFormat::Anthropic,
        _ => qb_station::station::pricing::PricingFormat::OpenAi,
    };
    let got =
        qb_station::station::pricing::parse_pricing_markdown(&text, "2026-09-14", "live", fmt);
    println!("解析出 {} 个模型：", got.len());
    for f in got.iter().take(12) {
        println!(
            "  {:<24} 输入 {:>8.3}  输出 {:>8.3}  缓存读 {:>8.3?}  缓存写 {:>8.3?}  长上下文 {}",
            f.model,
            f.input_per_mtok,
            f.output_per_mtok,
            f.cache_read_per_mtok,
            f.cache_write_per_mtok,
            f.long_context.map_or("无".into(), |l| format!(
                "输入 {} 输出 {}",
                l.input_per_mtok, l.output_per_mtok
            )),
        );
    }
}
