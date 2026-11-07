//! 首次运行时的演示数据。
//!
//! 没有任何数据文件时初始化三个账号、常用分类与几件商品，便于直接体验。
//! 密码同样经过加盐散列存储，与注册流程一致。

use rust_decimal::Decimal;

use crate::domain::item::{Category, Item, ItemStatus};
use crate::domain::user::User;
use crate::repo::Store;
use crate::security;

/// 演示账号：`admin / admin123`、`seller / seller123`、`buyer / buyer123`。
pub const DEMO_ACCOUNTS: [(&str, &str); 3] = [
    ("admin", "admin123"),
    ("seller", "seller123"),
    ("buyer", "buyer123"),
];

const CATEGORIES: [&str; 5] = ["教材书籍", "数码电子", "生活用品", "运动器材", "其他"];

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

    let seller_id = seller.user_id.clone();
    store.users.extend([admin, seller, buyer]);

    for name in CATEGORIES {
        store.categories.push(Category::new(name));
    }

    // 已发布商品：买家登录后可直接浏览、预约（REQ-015）。
    demo_item(
        &mut store,
        &seller_id,
        0,
        "高等数学教材（第七版）",
        "笔记完整，无破损",
        25,
        ItemStatus::Published,
    );
    demo_item(
        &mut store,
        &seller_id,
        1,
        "罗技无线鼠标 M330",
        "用了一学期，功能正常",
        45,
        ItemStatus::Published,
    );
    demo_item(
        &mut store,
        &seller_id,
        2,
        "宿舍护眼台灯",
        "亮度三档，附原装电源",
        15,
        ItemStatus::Published,
    );
    // 待审核商品：管理员登录后可直接体验审核流程（REQ-006）。
    demo_item(
        &mut store,
        &seller_id,
        3,
        "二手篮球（7 号）",
        "气足，适合练习",
        20,
        ItemStatus::Pending,
    );

    store
}

fn demo_item(
    store: &mut Store,
    seller_id: &str,
    category_index: usize,
    title: &str,
    description: &str,
    price: i64,
    status: ItemStatus,
) {
    let category_id = store.categories[category_index].category_id.clone();
    let mut item = Item::new(seller_id, category_id);
    item.title = title.to_string();
    item.description = description.to_string();
    item.price = Decimal::new(price, 0);
    item.status = status;
    store.items.push(item);
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
        assert_eq!(store.items.len(), 4);
        assert_eq!(
            store
                .items
                .iter()
                .filter(|item| item.status == ItemStatus::Published)
                .count(),
            3
        );
    }
}
