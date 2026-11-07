//! 首次运行时的演示数据。
//!
//! 没有任何数据文件时初始化三个账号与常用分类，便于直接登录体验。
//! 密码同样经过加盐散列存储，与注册流程一致。

use crate::domain::item::Category;
use crate::domain::user::User;
use crate::repo::Store;
use crate::security;

/// 演示账号：`admin / admin123`、`seller / seller123`、`buyer / buyer123`。
pub const DEMO_ACCOUNTS: [(&str, &str); 3] = [
    ("admin", "admin123"),
    ("seller", "seller123"),
    ("buyer", "buyer123"),
];

/// 初始化演示数据。
pub fn demo_store() -> Store {
    let mut store = Store::new();

    // 管理员：仅具备管理员身份（REQ-003 权限隔离）。
    let mut admin = User::new(
        "admin",
        Some("13800000000".into()),
        Some("admin@example.com".into()),
        security::hash_password("admin123"),
    );
    admin.buyer_role = false;
    admin.seller_role = false;
    admin.admin_role = true;

    // 普通账号：注册后默认同时具备买家与卖家身份（REQ-001）。
    let seller = User::new(
        "seller",
        Some("13800000001".into()),
        Some("seller@example.com".into()),
        security::hash_password("seller123"),
    );
    let buyer = User::new(
        "buyer",
        Some("13800000002".into()),
        Some("buyer@example.com".into()),
        security::hash_password("buyer123"),
    );

    store.users.extend([admin, seller, buyer]);

    for name in ["教材书籍", "数码电子", "生活用品", "运动器材", "其他"] {
        store.categories.push(Category::new(name));
    }

    store
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_accounts_can_login_with_initial_passwords() {
        let store = demo_store();
        for (username, password) in DEMO_ACCOUNTS {
            let user = store
                .find_user_by_username(username)
                .unwrap_or_else(|| panic!("缺少演示账号 {username}"));
            assert!(
                security::verify_password(password, &user.password_hash),
                "演示账号 {username} 密码校验失败"
            );
        }
        assert_eq!(store.users.len(), DEMO_ACCOUNTS.len());
        assert!(!store.categories.is_empty());
    }
}
