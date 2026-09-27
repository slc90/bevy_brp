use bevy::prelude::*;
use bevy_brp_extras::BrpExtrasActivityState;
use std::sync::{Arc, Mutex, MutexGuard};

const TAIL_UPDATE_BUDGET: u8 = 2;

/// Main 与 Render World 共享的 BRP 按需续帧状态。
#[derive(Clone, Default)]
pub(super) struct BrpProgress {
    inner: Arc<Mutex<ProgressState>>,
}

/// Main World 每次完成 update 后需要执行的有界推进动作。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct FrameAction {
    /// 是否需要通过 `RequestRedraw` 请求下一次正常 update。
    pub(super) request_redraw: bool,
    /// 当前唯一有效的 fallback generation。
    pub(super) fallback_generation: Option<u64>,
}

/// 线程安全 state 的 World mailbox 来源。
#[derive(Clone, Copy)]
pub(super) enum MailboxWorld {
    /// Main World 的 BRP mailbox。
    Main,
    /// Render World 的 BRP mailbox。
    Render,
}

/// 汇总所有真实工作来源、收尾 budget 与 fallback generation。
#[derive(Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent work sources are intentionally tracked as orthogonal flags"
)]
struct ProgressState {
    generation: u64,
    fallback_generation: Option<u64>,
    extras_generation: u64,
    extras_initialized: bool,
    extras_active: bool,
    main_mailbox_pending: bool,
    render_mailbox_pending: bool,
    pending_results: usize,
    tail_updates: u8,
    shutdown: bool,
}

impl BrpProgress {
    /// 登记一个从成功入队到首个 BRP result 的普通请求等待。
    pub(super) fn begin_result_wait(&self) {
        let mut state = lock_state(&self.inner);
        state.pending_results = state.pending_results.saturating_add(1);
        state.advance_generation();
    }

    /// 结束普通请求等待，并合并最多两帧的收尾机会。
    pub(super) fn finish_result_wait(&self) {
        let mut state = lock_state(&self.inner);
        let Some(pending_results) = state.pending_results.checked_sub(1) else {
            error!("BRP runtime pending result guard 发生计数下溢");
            return;
        };
        state.pending_results = pending_results;
        state.add_tail_updates();
        state.advance_generation();
    }

    /// 按 Extras generation 接受最新状态，忽略晚到的旧通知。
    pub(super) fn update_extras(&self, activity: BrpExtrasActivityState) -> bool {
        self.update_extras_state(activity.generation(), activity.is_active())
    }

    /// 报告 Render mailbox 残余工作；状态变化后由调用方立即 wake Main event loop。
    pub(super) fn report_render_mailbox(&self, pending: bool) -> bool {
        self.report_mailbox(MailboxWorld::Render, pending)
    }

    /// Main World 在 Remote cleanup 后确认本次 update，并决定是否继续推进。
    #[allow(
        clippy::significant_drop_tightening,
        reason = "the state lock intentionally covers the complete atomic frame decision"
    )]
    pub(super) fn finish_main_update(&self, main_mailbox_pending: bool) -> FrameAction {
        let mut state = lock_state(&self.inner);
        state.set_mailbox(MailboxWorld::Main, main_mailbox_pending);
        state.advance_generation();
        state.fallback_generation = None;

        if state.shutdown {
            return FrameAction {
                request_redraw: false,
                fallback_generation: None,
            };
        }

        let real_work = state.has_real_work();
        if !real_work && state.tail_updates != 0 {
            state.tail_updates -= 1;
        }
        let continue_updates = real_work || state.tail_updates != 0;
        let fallback_generation = continue_updates.then_some(state.generation);
        state.fallback_generation = fallback_generation;
        FrameAction {
            request_redraw: continue_updates,
            fallback_generation,
        }
    }

    /// fallback timer 到期时只允许当前 generation 发出一次 `WakeUp`。
    pub(super) fn fire_fallback(&self, generation: u64) -> bool {
        let mut state = lock_state(&self.inner);
        if state.shutdown || state.fallback_generation != Some(generation) {
            return false;
        }
        state.fallback_generation = None;
        true
    }

    /// `AppExit` 时使全部后续通知和 timer 静默失效。
    pub(super) fn shutdown(&self) {
        let mut state = lock_state(&self.inner);
        state.shutdown = true;
        state.fallback_generation = None;
        state.advance_generation();
    }

    /// 测试与 Extras callback 共用的 generation 更新实现。
    fn update_extras_state(&self, generation: u64, active: bool) -> bool {
        let mut state = lock_state(&self.inner);
        if state.shutdown || generation <= state.extras_generation {
            return false;
        }
        let was_initialized = state.extras_initialized;
        let was_active = state.extras_active;
        state.extras_generation = generation;
        state.extras_initialized = true;
        state.extras_active = active;
        if was_initialized && was_active && !active {
            state.add_tail_updates();
        }
        state.advance_generation();
        true
    }

    /// 更新指定 World 的 mailbox flag，并在 work 结束时合并收尾 budget。
    fn report_mailbox(&self, world: MailboxWorld, pending: bool) -> bool {
        let mut state = lock_state(&self.inner);
        if state.shutdown {
            return false;
        }
        let changed = state.set_mailbox(world, pending);
        if changed {
            state.advance_generation();
        }
        changed
    }
}

impl ProgressState {
    /// 推进 controller generation，使此前 timer 自动变成 stale。
    const fn advance_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.fallback_generation = None;
    }

    /// 合并收尾需求的上界，不因重复通知累加。
    fn add_tail_updates(&mut self) {
        self.tail_updates = self.tail_updates.max(TAIL_UPDATE_BUDGET);
    }

    /// 返回 mailbox、普通请求或 Extras 是否仍有真实工作。
    const fn has_real_work(&self) -> bool {
        self.main_mailbox_pending
            || self.render_mailbox_pending
            || self.pending_results != 0
            || self.extras_active
    }

    /// 设置一个 World 的 mailbox flag，并返回是否发生变化。
    fn set_mailbox(&mut self, world: MailboxWorld, pending: bool) -> bool {
        let previous = match world {
            MailboxWorld::Main => {
                let previous = self.main_mailbox_pending;
                self.main_mailbox_pending = pending;
                previous
            }
            MailboxWorld::Render => {
                let previous = self.render_mailbox_pending;
                self.render_mailbox_pending = pending;
                previous
            }
        };
        if previous && !pending {
            self.add_tail_updates();
        }
        previous != pending
    }
}

/// mutex poisoned 时保留 controller state，并输出明确诊断。
fn lock_state(state: &Mutex<ProgressState>) -> MutexGuard<'_, ProgressState> {
    match state.lock() {
        Ok(state) => state,
        Err(poisoned) => {
            error!("BRP runtime progress state mutex 已损坏");
            poisoned.into_inner()
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "tests assert that busy progress always produces a fallback generation"
)]
mod tests {
    use super::*;

    /// 最后一项普通请求完成后只允许两个完整 update，之后恢复 idle。
    #[test]
    fn result_completion_has_bounded_tail_updates() {
        let progress = BrpProgress::default();
        progress.begin_result_wait();
        let busy = progress.finish_main_update(false);
        progress.finish_result_wait();

        let first_tail = progress.finish_main_update(false);
        let second_tail = progress.finish_main_update(false);

        assert!(busy.request_redraw);
        assert!(first_tail.request_redraw);
        assert!(!second_tail.request_redraw);
        assert!(second_tail.fallback_generation.is_none());
    }

    /// 并发请求中先完成的一项不能清除另一项仍持有的工作责任。
    #[test]
    fn concurrent_result_waits_remain_busy_until_both_finish() {
        let progress = BrpProgress::default();
        progress.begin_result_wait();
        progress.begin_result_wait();
        progress.finish_result_wait();

        assert!(progress.finish_main_update(false).request_redraw);
        progress.finish_result_wait();
        assert!(progress.finish_main_update(false).request_redraw);
        assert!(!progress.finish_main_update(false).request_redraw);
    }

    /// 旧 Extras generation 不得覆盖更晚到达的 idle state。
    #[test]
    fn stale_extras_notification_cannot_restore_busy() {
        let progress = BrpProgress::default();

        assert!(progress.update_extras_state(1, true));
        assert!(progress.update_extras_state(3, false));
        assert!(!progress.update_extras_state(2, true));
        assert!(progress.finish_main_update(false).request_redraw);
        assert!(!progress.finish_main_update(false).request_redraw);
    }

    /// fallback generation 只能成功触发一次，后续重复或旧 timer 都保持静默。
    #[test]
    fn fallback_fires_once_for_the_confirmed_generation() {
        let progress = BrpProgress::default();
        progress.begin_result_wait();
        let action = progress.finish_main_update(false);
        let generation = action
            .fallback_generation
            .expect("busy frame should schedule fallback");

        assert!(progress.fire_fallback(generation));
        assert!(!progress.fire_fallback(generation));
        let replacement = progress.finish_main_update(false);
        assert_ne!(replacement.fallback_generation, Some(generation));
    }

    /// timer 预约后到达的更新通知会使旧 generation 失效，不能覆盖较新的 state。
    #[test]
    fn progress_change_invalidates_older_fallback() {
        let progress = BrpProgress::default();
        progress.begin_result_wait();
        let generation = progress
            .finish_main_update(false)
            .fallback_generation
            .expect("busy frame should schedule fallback");

        progress.finish_result_wait();

        assert!(!progress.fire_fallback(generation));
    }

    /// Render mailbox 在 Main 判断后出现时会报告状态变化，供调用方补发 wake。
    #[test]
    fn render_mailbox_change_is_visible_across_worlds() {
        let progress = BrpProgress::default();
        assert!(!progress.finish_main_update(false).request_redraw);

        assert!(progress.report_render_mailbox(true));
        assert!(progress.finish_main_update(false).request_redraw);
        assert!(progress.report_render_mailbox(false));
    }
}
