import type { DemoCase } from '../types';

let input = 'use std::collections::HashMap;\nuse serde::Serialize;\n\n';
input += 'pub struct Config {\n    pub name: String,\n    pub retries: u32,\n}\n\n';
input += 'pub fn build_index(cfg: &Config, entries: &[String]) -> HashMap<String, usize> {\n';
for (let i = 0; i < 30; i++) {
  input += `    let slot_${i} = cfg.name.clone() + "_${i}";\n`;
  input += `    consume(&slot_${i});\n`;
}
input += '    let mut map = HashMap::new();\n';
input += '    map.insert(cfg.name.clone(), cfg.retries as usize);\n';
input += '    map\n}\n';

export const sourceCodeCase: DemoCase = {
  id: 'source-code',
  title: 'Rust 源代码',
  description: '识别 Rust 结构并折叠长函数体，保留签名，完整源码写入 stash。',
  input,
  expectedType: 'source_code',
  expectedPath: 'lossy-stash',
  mustContain: ['pub struct Config', 'pub fn build_index', 'lines omitted'],
};
