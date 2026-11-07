//! Una sola instancia por sesion Windows, incluso entre debug y release.

use std::io;
use std::ptr::null;
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows_sys::Win32::System::Threading::CreateMutexW;

pub struct Guard(HANDLE);

impl Guard {
    pub fn acquire() -> io::Result<Option<Self>> {
        let name: Vec<u16> = "Local\\EB1TR.CATPacer"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: nombre terminado en cero; no se hereda el handle ni se toma propiedad del mutex.
        let handle = unsafe { CreateMutexW(null(), 0, name.as_ptr()) };
        // Debe leerse antes de cualquier otra llamada Win32.
        let error = unsafe { GetLastError() };
        if handle == 0 {
            return Err(io::Error::from_raw_os_error(error as i32));
        }
        let guard = Self(handle);
        if error == ERROR_ALREADY_EXISTS {
            return Ok(None);
        }
        Ok(Some(guard))
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        // SAFETY: el handle pertenece a este objeto; Windows elimina el mutex al cerrar el ultimo.
        unsafe { CloseHandle(self.0) };
    }
}
