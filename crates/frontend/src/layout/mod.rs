pub mod center;
pub mod footer;
pub mod global_context;
pub mod header;
pub mod left;
pub mod right;
pub mod tabs;
pub mod top_header;

use crate::system::maintenance::MaintenanceNotice;
use leptos::prelude::*;
use top_header::TopHeader;

/// Main application shell with new UI design structure.
///
/// Layout structure (matching bolt-mpi-ui-redesign):
/// ```text
/// +------------------------------------------+
/// |              TopHeader                    |
/// +------------------------------------------+
/// |  Sidebar  |    Content    |  RightPanel  |
/// |   (Left)  |   (Center)    |   (Right)    |
/// +------------------------------------------+
/// ```
///
/// Note: No separate Header or Footer - TopHeader handles all top bar functionality.
#[component]
pub fn Shell<L, C, R>(left: L, center: C, right: R) -> impl IntoView
where
    L: Fn() -> AnyView + 'static + Send,
    C: Fn() -> AnyView + 'static + Send,
    R: Fn() -> AnyView + 'static + Send,
{
    // Note: Left/Right components get AppGlobalContext internally
    // for sidebar/panel visibility control

    view! {
        <div class="app-layout">
            // Внутрь приложения при обслуживании пущен только администратор —
            // он должен видеть, что для остальных оно закрыто. Плашка стоит
            // первым flex-элементом колонки, поэтому просто ужимает остальное.
            <MaintenanceNotice />

            // Top header with toggle controls
            <TopHeader />

            // Main body with sidebar, content, and right panel
            <div class="app-body">
                // Left sidebar - uses ctx.left_open for visibility
                <left::Left>
                    {left()}
                </left::Left>

                // Main content area
                <div class="app-main">
                    <center::Center>
                        {center()}
                    </center::Center>
                </div>

                // Right panel - uses ctx.right_open for visibility
                <right::Right>
                    {right()}
                </right::Right>
            </div>

        </div>
    }
}

/// Legacy Shell with old design (Header + Footer)
/// Use this for backward compatibility if needed
#[component]
pub fn ShellLegacy<L, C, R>(left: L, center: C, right: R) -> impl IntoView
where
    L: Fn() -> AnyView + 'static + Send,
    C: Fn() -> AnyView + 'static + Send,
    R: Fn() -> AnyView + 'static + Send,
{
    view! {
        <header::Header />
        <div class="main-content">
            <left::Left>
                {left()}
            </left::Left>
            <center::Center>
                {center()}
            </center::Center>
            <right::Right>
                {right()}
            </right::Right>
        </div>
        <footer::Footer />
    }
}
