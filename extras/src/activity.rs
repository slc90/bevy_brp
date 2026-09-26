//! BRP Extras 跨帧操作的活动状态。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use bevy::prelude::*;

type ActivityCallback = Arc<dyn Fn(BrpExtrasActivityState) + Send + Sync + 'static>;

/// BRP Extras 当前活动状态的只读快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrpExtrasActivityState {
    generation: u64,
    active_count: usize,
}

/// 汇总 BRP Extras 仍需 App update 推进的真实操作。
///
/// 宿主可以读取当前状态并安装通知 callback；操作 ownership 由 crate 内部 guard 管理。
#[derive(Resource, Default)]
pub struct BrpExtrasActivity {
    inner: Arc<Mutex<ActivityInner>>,
}

/// 由真实异步操作持有的活动责任，drop 时自动释放。
pub(crate) struct BrpExtrasActivityGuard {
    lease: Arc<ActivityLease>,
}

/// 允许 operation owner 在 worker 尚未返回时幂等取消同一项活动责任。
#[derive(Clone)]
pub(crate) struct BrpExtrasActivityCancellation {
    lease: Arc<ActivityLease>,
}

/// 把一次计数 ownership 与可跨线程取消的 released flag 绑定。
struct ActivityLease {
    inner: Arc<Mutex<ActivityInner>>,
    released: AtomicBool,
}

/// 允许异步 worker 在不改变活动计数时通知宿主重新检查结果。
#[derive(Clone)]
pub(crate) struct BrpExtrasActivityNotifier {
    inner: Arc<Mutex<ActivityInner>>,
}

/// 在 mutex 内保存计数、通知序号与 callback，callback 始终在解锁后调用。
#[derive(Default)]
struct ActivityInner {
    generation: u64,
    active_count: usize,
    callback: Option<ActivityCallback>,
}

impl BrpExtrasActivityState {
    /// 返回该快照是否仍有真实操作需要推进。
    #[must_use]
    pub const fn is_active(self) -> bool {
        self.active_count != 0
    }

    /// 返回快照中的并发活动操作数量。
    #[must_use]
    pub const fn active_count(self) -> usize {
        self.active_count
    }

    /// 返回通知 generation，宿主可用它忽略乱序到达的旧通知。
    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }
}

impl BrpExtrasActivity {
    /// 返回当前活动状态；读取不会触发通知。
    #[must_use]
    pub fn state(&self) -> BrpExtrasActivityState {
        let inner = lock_inner(&self.inner);
        inner.state()
    }

    /// 安装状态通知 callback，并立即以新的 generation 交付当前状态。
    ///
    /// callback 可能从 App worker thread 调用，且不会在内部 mutex 持有期间调用。
    pub fn set_change_callback(
        &self,
        callback: impl Fn(BrpExtrasActivityState) + Send + Sync + 'static,
    ) {
        let callback: ActivityCallback = Arc::new(callback);
        let state = {
            let mut inner = lock_inner(&self.inner);
            inner.callback = Some(callback.clone());
            inner.next_state()
        };
        callback(state);
    }

    /// 为新建跨帧操作取得一项活动责任。
    pub(crate) fn begin(&self) -> BrpExtrasActivityGuard {
        let notification = {
            let mut inner = lock_inner(&self.inner);
            let was_idle = inner.active_count == 0;
            inner.active_count = inner.active_count.saturating_add(1);
            let notification = was_idle.then(|| inner.notification());
            drop(inner);
            notification
        };
        notify(notification);
        BrpExtrasActivityGuard {
            lease: Arc::new(ActivityLease {
                inner: self.inner.clone(),
                released: AtomicBool::new(false),
            }),
        }
    }
}

impl BrpExtrasActivityGuard {
    /// 创建不拥有活动计数的 worker 通知 handle。
    pub(crate) fn notifier(&self) -> BrpExtrasActivityNotifier {
        BrpExtrasActivityNotifier {
            inner: self.lease.inner.clone(),
        }
    }

    /// 创建可由原 operation owner 持有的幂等取消 handle。
    pub(crate) fn cancellation(&self) -> BrpExtrasActivityCancellation {
        BrpExtrasActivityCancellation {
            lease: self.lease.clone(),
        }
    }
}

impl Drop for BrpExtrasActivityGuard {
    fn drop(&mut self) {
        self.lease.release();
    }
}

impl BrpExtrasActivityCancellation {
    /// 提前释放 guard 代表的活动责任；worker 后续 drop 不会重复递减。
    pub(crate) fn cancel(&self) {
        self.lease.release();
    }
}

impl ActivityLease {
    /// 只允许最先到达的取消或 drop 释放活动计数。
    fn release(&self) {
        if self
            .released
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let notification = {
            let mut inner = lock_inner(&self.inner);
            let Some(active_count) = inner.active_count.checked_sub(1) else {
                error!("BRP Extras activity guard 发生计数下溢");
                return;
            };
            inner.active_count = active_count;
            let notification = (active_count == 0).then(|| inner.notification());
            drop(inner);
            notification
        };
        notify(notification);
    }
}

impl BrpExtrasActivityNotifier {
    /// 通知宿主异步进度已经可供 World 消费。
    pub(crate) fn notify_progress(&self) {
        let notification = {
            let mut inner = lock_inner(&self.inner);
            inner.notification()
        };
        notify(Some(notification));
    }
}

impl ActivityInner {
    /// 返回 mutex 内当前状态。
    const fn state(&self) -> BrpExtrasActivityState {
        BrpExtrasActivityState {
            generation: self.generation,
            active_count: self.active_count,
        }
    }

    /// 推进 generation 并返回最新状态。
    const fn next_state(&mut self) -> BrpExtrasActivityState {
        self.generation = self.generation.wrapping_add(1);
        self.state()
    }

    /// 生成解锁后调用所需的 callback 与快照。
    fn notification(&mut self) -> (Option<ActivityCallback>, BrpExtrasActivityState) {
        let state = self.next_state();
        (self.callback.clone(), state)
    }
}

/// mutex poisoned 时保留现有 state，并输出明确诊断。
fn lock_inner(inner: &Mutex<ActivityInner>) -> MutexGuard<'_, ActivityInner> {
    match inner.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            error!("BRP Extras activity state mutex 已损坏");
            poisoned.into_inner()
        }
    }
}

/// 在 mutex 外调用宿主 callback，避免 callback 重入造成 deadlock。
fn notify(notification: Option<(Option<ActivityCallback>, BrpExtrasActivityState)>) {
    let Some((Some(callback), state)) = notification else {
        return;
    };
    callback(state);
}

/// 取得活动 Resource，并兼容直接调用 handler 的最小测试 World。
pub(crate) fn begin(world: &mut World) -> BrpExtrasActivityGuard {
    world.init_resource::<BrpExtrasActivity>();
    world.resource::<BrpExtrasActivity>().begin()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// 多项并发 guard 只在 idle/busy 边界通知，最后一项释放后恢复 idle。
    #[test]
    fn concurrent_guards_preserve_busy_until_the_last_drop() {
        let activity = BrpExtrasActivity::default();
        let states = Arc::new(Mutex::new(Vec::new()));
        let observed = states.clone();
        activity.set_change_callback(move |state| {
            if let Ok(mut states) = observed.lock() {
                states.push(state);
            }
        });

        let first = activity.begin();
        let second = activity.begin();
        drop(first);
        assert!(activity.state().is_active());
        drop(second);

        let states = match states.lock() {
            Ok(states) => states,
            Err(poisoned) => poisoned.into_inner(),
        };
        assert_eq!(states.len(), 3);
        assert!(!states[0].is_active());
        assert_eq!(states[1].active_count(), 1);
        assert!(!states[2].is_active());
        assert!(states[0].generation() < states[1].generation());
        assert!(states[1].generation() < states[2].generation());
        drop(states);
    }

    /// worker 进度通知保持活动计数不变，同时产生更新的 generation。
    #[test]
    fn progress_notification_preserves_activity_ownership() {
        let activity = BrpExtrasActivity::default();
        let states = Arc::new(Mutex::new(Vec::new()));
        let observed = states.clone();
        activity.set_change_callback(move |state| {
            if let Ok(mut states) = observed.lock() {
                states.push(state);
            }
        });
        let guard = activity.begin();
        let notifier = guard.notifier();

        notifier.notify_progress();

        assert_eq!(activity.state().active_count(), 1);
        drop(guard);
        let states = match states.lock() {
            Ok(states) => states,
            Err(poisoned) => poisoned.into_inner(),
        };
        assert_eq!(states.len(), 4);
        assert_eq!(states[2].active_count(), 1);
        assert!(states[1].generation() < states[2].generation());
        drop(states);
    }

    /// operation owner 取消已转移给 worker 的 guard 时立即恢复 idle，后续 drop 不重复释放。
    #[test]
    fn cancellation_releases_transferred_guard_once() {
        let activity = BrpExtrasActivity::default();
        let guard = activity.begin();
        let cancellation = guard.cancellation();

        cancellation.cancel();
        assert!(!activity.state().is_active());
        drop(guard);

        assert_eq!(activity.state().active_count(), 0);
    }
}
