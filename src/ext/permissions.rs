use deno_core::url::Url;
use deno_permissions::PermissionCheckError;
use deno_permissions::PermissionDeniedError;
use std::borrow::Cow;
use std::path::Path;

#[derive(Clone)]
pub struct Permissions {}

impl deno_web::TimersPermission for Permissions {
    fn allow_hrtime(&mut self) -> bool {
        false
    }
}

impl deno_fetch::FetchPermissions for Permissions {
    fn check_net(
        &mut self,
        _host: &str,
        _port: u16,
        _api_name: &str,
    ) -> Result<(), PermissionCheckError> {
        Ok(()) // TODO: implement proper permission check
    }

    fn check_net_url(&mut self, _url: &Url, _api_name: &str) -> Result<(), PermissionCheckError> {
        Ok(()) // TODO: implement proper permission check
    }

    fn check_open<'a>(
        &mut self,
        path: Cow<'a, Path>,
        _open_access: deno_permissions::OpenAccessKind,
        _api_name: &str,
    ) -> Result<deno_permissions::CheckedPath<'a>, PermissionCheckError> {
        // Deny file access by default
        Err(PermissionCheckError::PermissionDenied(
            PermissionDeniedError {
                access: format!("File access not allowed: {:?}", path.display()),
                name: "read",
                custom_message: None,
            },
        ))
    }

    fn check_net_vsock(
        &mut self,
        _cid: u32,
        _port: u32,
        _api_name: &str,
    ) -> Result<(), PermissionCheckError> {
        Err(PermissionCheckError::PermissionDenied(
            PermissionDeniedError {
                access: "VSOCK access not allowed".to_string(),
                name: "net",
                custom_message: None,
            },
        ))
    }
}

deno_core::extension!(
    permissions,
    state = |state| state.put::<Permissions>(Permissions {})
);
