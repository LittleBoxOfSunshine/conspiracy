use conspiracy_macros::define_features;
use conspiracy_theories::config::RestartRequired;

define_features!(
    pub enum Features {
        #[conspiracy(restart)]
        Foo => false,
        Bar => false,
    }
);

define_features!(
    pub enum OneRequiresRestart {
        #[conspiracy(restart)]
        Foo => false,
        Bar => false,
    }
);

define_features!(
    pub enum SomeRequireRestart {
        #[conspiracy(restart)]
        Foo => false,
        Bar => false,
        #[conspiracy(restart)]
        Cow => true,
    }
);

define_features!(
    pub enum AllRequireRestart {
        #[conspiracy(restart)]
        Foo => false,
        #[conspiracy(restart)]
        Bar => false,
        #[conspiracy(restart)]
        Cow => true,
    }
);

#[test]
fn no_change_no_restart() {
    assert!(
        !OneRequiresRestartState::default().restart_required(&OneRequiresRestartState::default())
    );
    assert!(
        !SomeRequireRestartState::default().restart_required(&SomeRequireRestartState::default())
    );
    assert!(!AllRequireRestartState::default().restart_required(&AllRequireRestartState::default()));
}

#[test]
fn untracked_change_no_restart() {
    let mut other = OneRequiresRestartState::default();
    other.bar = !other.bar;
    assert!(!OneRequiresRestartState::default().restart_required(&other));

    let mut other = SomeRequireRestartState::default();
    other.bar = !other.bar;
    assert!(!SomeRequireRestartState::default().restart_required(&other));
}

#[test]
fn tracked_change_restart() {
    let mut other = OneRequiresRestartState::default();
    other.foo = !other.foo;
    assert!(OneRequiresRestartState::default().restart_required(&other));

    let mut other = SomeRequireRestartState::default();
    other.cow = !other.cow;
    assert!(SomeRequireRestartState::default().restart_required(&other));

    let mut other = SomeRequireRestartState::default();
    other.bar = !other.bar;
    assert!(!SomeRequireRestartState::default().restart_required(&other));
    other.cow = !other.cow;
    assert!(SomeRequireRestartState::default().restart_required(&other));

    let mut other = AllRequireRestartState::default();
    other.bar = !other.bar;
    other.cow = !other.cow;
    assert!(AllRequireRestartState::default().restart_required(&other));
}
