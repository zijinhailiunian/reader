//! 音色列表的进程内缓存（Edge 与微软朗读共用）。
//!
//! 两个通道都是「拉一次列表 → 用户挑一个音色」，列表本身几分钟内不会变，所以
//! 缓存在进程里。早先的实现只缓存不设期限：**服务端加了新音色，用户必须重启
//! 应用才看得到**，而这两个通道的音色集合确实在陆续增加。
//!
//! 这里同时保留「过期」与「失效」两种状态：超过 TTL 后重新拉一次，拉取失败就
//! 退回旧列表（并记一条 warn），而不是让设置页突然变成空下拉框 —— 断网时旧列表
//! 里已有的音色照样能用。

use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;

/// 音色列表的缓存时长。一天足够；再短就得为一个几乎静态的列表反复打网络。
pub const VOICE_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// 带 `fetched_at` 的列表缓存。`None` 表示还没成功拉过。
type Slot = Mutex<Option<(Instant, Vec<Value>)>>;

/// 一个通道（Edge / 微软）各自的缓存实例。
pub struct VoiceCache {
    slot: OnceLock<Slot>,
}

impl VoiceCache {
    pub const fn new() -> Self {
        Self {
            slot: OnceLock::new(),
        }
    }

    fn slot(&self) -> &Slot {
        self.slot.get_or_init(|| Mutex::new(None))
    }

    /// 未过期的列表。
    pub fn fresh(&self) -> Option<Vec<Value>> {
        self.fresh_at(Instant::now())
    }

    /// 任意一份旧列表（可能已过期）。重新拉取失败时用作兜底。
    pub fn any(&self) -> Option<Vec<Value>> {
        self.slot()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
            .map(|(_, voices)| voices.clone())
    }

    pub fn store(&self, voices: Vec<Value>) {
        *self.slot()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((Instant::now(), voices));
    }

    /// `fresh()` 的可注入版本：单测用它验证过期边界，不必真的 sleep 一天。
    fn fresh_at(&self, now: Instant) -> Option<Vec<Value>> {
        let guard = self
            .slot()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (fetched_at, voices) = guard.as_ref()?;
        is_fresh(*fetched_at, now).then(|| voices.clone())
    }
}

/// 缓存是否还在有效期内。单独提出来是为了单测能注入时间点。
pub fn is_fresh(fetched_at: Instant, now: Instant) -> bool {
    now.saturating_duration_since(fetched_at) < VOICE_CACHE_TTL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ttl_boundary() {
        // 只做**向前**的时间算术：`Instant - Duration` 在机器开机时长小于 Duration 时
        // 会 underflow 直接 panic（Windows 的 Instant 基于开机以来的计数，刚重启的机器
        // 与 CI runner 上必现），所以这里把"现在"往前推，而不是把 `fetched_at` 往回调。
        let fetched_at = Instant::now();
        assert!(is_fresh(fetched_at, fetched_at), "刚拉到的必然是新鲜的");
        assert!(is_fresh(
            fetched_at,
            fetched_at + (VOICE_CACHE_TTL - Duration::from_secs(1))
        ));
        // 到点即过期：等于 TTL 也算过期，否则「一天」会变成「一天零一点」。
        assert!(!is_fresh(fetched_at, fetched_at + VOICE_CACHE_TTL));
        assert!(!is_fresh(
            fetched_at,
            fetched_at + (VOICE_CACHE_TTL + Duration::from_secs(1))
        ));
    }

    #[test]
    fn empty_cache_returns_none() {
        let cache = VoiceCache::new();
        assert!(cache.fresh().is_none());
        assert!(cache.any().is_none());
    }

    #[test]
    fn fresh_returns_value_and_expired_falls_back_to_any() {
        let cache = VoiceCache::new();
        let voices = vec![Value::String("zh-CN-XiaoxiaoNeural".to_string())];

        cache.store(voices.clone());
        assert_eq!(cache.fresh(), Some(voices.clone()));
        assert_eq!(cache.any(), Some(voices.clone()));

        // 手动把"现在"推到 TTL 之外：`fresh` 不再给值，但 `any` 仍能兜底。
        // 同样只做向前算术（见 `ttl_boundary`），`fresh_at` 就是为此预留的注入口。
        let later = Instant::now() + VOICE_CACHE_TTL;
        assert!(cache.fresh_at(later).is_none());
        assert_eq!(cache.any(), Some(voices));
    }
}
