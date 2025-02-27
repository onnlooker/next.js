use std::io::Write;

use anyhow::{bail, Result};
use indexmap::indexmap;
use indoc::writedoc;
use serde::Serialize;
use turbo_tasks::{Value, ValueToString};
use turbopack_binding::{
    turbo::tasks_fs::{rope::RopeBuilder, File, FileSystemPathVc},
    turbopack::{
        core::{
            asset::Asset,
            context::AssetContext,
            reference_type::{EcmaScriptModulesReferenceSubType, InnerAssetsVc, ReferenceType},
            source::SourceVc,
            virtual_source::VirtualSourceVc,
        },
        ecmascript::{chunk::EcmascriptChunkPlaceableVc, utils::StringifyJs},
        turbopack::ModuleAssetContextVc,
    },
};

use super::app_entries::{AppEntry, AppEntryVc};

/// Computes the entry for a Next.js app route.
pub(super) async fn get_app_route_entry(
    rsc_context: ModuleAssetContextVc,
    source: SourceVc,
    pathname: &str,
    project_root: FileSystemPathVc,
) -> Result<AppEntryVc> {
    let mut result = RopeBuilder::default();

    let kind = "app-route";
    let original_name = get_original_route_name(pathname);
    let path = source.ident().path();

    let options = AppRouteRouteModuleOptions {
        definition: AppRouteRouteDefinition {
            kind: RouteKind::AppRoute,
            page: original_name.clone(),
            pathname: pathname.to_string(),
            filename: path.file_stem().await?.as_ref().unwrap().clone(),
            // TODO(alexkirsz) Is this necessary?
            bundle_path: "".to_string(),
        },
        resolved_page_path: path.to_string().await?.clone_value(),
        // TODO(alexkirsz) Is this necessary?
        next_config_output: "".to_string(),
    };

    // NOTE(alexkirsz) Keep in sync with
    // next.js/packages/next/src/build/webpack/loaders/next-app-loader.ts
    // TODO(alexkirsz) Support custom global error.
    writedoc!(
        result,
        r#"
            import 'next/dist/server/node-polyfill-headers'

            import RouteModule from {route_module}

            import * as userland from "ENTRY"

            const options = {options}
            const routeModule = new RouteModule({{
                ...options,
                userland,
            }})

            // Pull out the exports that we need to expose from the module. This should         
            // be eliminated when we've moved the other routes to the new format. These
            // are used to hook into the route.
            const {{
                requestAsyncStorage,
                staticGenerationAsyncStorage,
                serverHooks,
                headerHooks,
                staticGenerationBailout
            }} = routeModule

            const originalPathname = {original}

            export {{
                routeModule,
                requestAsyncStorage,
                staticGenerationAsyncStorage,
                serverHooks,
                headerHooks,
                staticGenerationBailout,
                originalPathname
            }}
        "#,
        route_module = StringifyJs(&format!(
            "next/dist/server/future/route-modules/{kind}/module"
        )),
        options = StringifyJs(&options),
        original = StringifyJs(&original_name),
    )?;

    let file = File::from(result.build());
    // TODO(alexkirsz) Figure out how to name this virtual asset.
    let virtual_source = VirtualSourceVc::new(project_root.join("todo.tsx"), file.into());

    let entry = rsc_context.process(
        source,
        Value::new(ReferenceType::EcmaScriptModules(
            EcmaScriptModulesReferenceSubType::Undefined,
        )),
    );

    let inner_assets = indexmap! {
        "ENTRY".to_string() => entry.into()
    };

    let rsc_entry = rsc_context.process(
        virtual_source.into(),
        Value::new(ReferenceType::Internal(InnerAssetsVc::cell(inner_assets))),
    );

    let Some(rsc_entry) = EcmascriptChunkPlaceableVc::resolve_from(rsc_entry).await? else {
        bail!("expected an ECMAScript chunk placeable asset");
    };

    Ok(AppEntry {
        pathname: pathname.to_string(),
        original_name,
        rsc_entry,
    }
    .cell())
}

fn get_original_route_name(pathname: &str) -> String {
    match pathname {
        "/" => "/route".to_string(),
        _ => format!("{}/route", pathname),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppRouteRouteModuleOptions {
    definition: AppRouteRouteDefinition,
    resolved_page_path: String,
    next_config_output: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppRouteRouteDefinition {
    kind: RouteKind,
    page: String,
    pathname: String,
    filename: String,
    bundle_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum RouteKind {
    AppRoute,
}
