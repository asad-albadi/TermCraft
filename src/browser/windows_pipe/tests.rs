use super::*;

#[test]
fn private_pipe_roundtrip_and_timeout() {
    let security = OwnerSecurity::new().unwrap();
    let (server, client) = pipe_pair(true, &security).unwrap();
    let cancel = AtomicBool::new(false);
    let mut output = [0u8; 16];
    let started = Instant::now();
    assert_eq!(
        server
            .transfer(
                IoBuffer::Read(&mut output),
                started + Duration::from_millis(25),
                &cancel
            )
            .unwrap(),
        None
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    let input = b"{\"id\":1}\0";
    let mut written = 0;
    assert_ne!(
        unsafe {
            WriteFile(
                client.as_raw_handle(),
                input.as_ptr(),
                input.len() as u32,
                &mut written,
                null_mut(),
            )
        },
        0
    );
    assert_eq!(written as usize, input.len());
    assert_eq!(
        server
            .transfer(
                IoBuffer::Read(&mut output),
                Instant::now() + Duration::from_secs(2),
                &cancel
            )
            .unwrap(),
        Some(input.len())
    );
    assert_eq!(&output[..input.len()], input);
}

#[test]
fn blocked_write_cancels_when_reader_stops_draining() {
    let security = OwnerSecurity::new().unwrap();
    let (server, _client) = pipe_pair(false, &security).unwrap();
    let cancel = AtomicBool::new(false);
    let started = Instant::now();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(25));
            cancel.store(true, Ordering::Release);
        });
        let input = vec![b'A'; 512 * 1024];
        let mut remaining = input.as_slice();
        let deadline = started + Duration::from_secs(2);
        while !remaining.is_empty() {
            let Some(written) = server
                .transfer(IoBuffer::Write(remaining), deadline, &cancel)
                .unwrap()
            else {
                break;
            };
            assert_ne!(written, 0);
            remaining = &remaining[written..];
        }
        assert!(!remaining.is_empty());
    });
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn closes_only_the_owned_job_and_profile() {
    let security = OwnerSecurity::new().unwrap();
    let first = Profile::new(&security).unwrap();
    let other = Profile::new(&security).unwrap();
    let owned_path = first.path.clone();
    let sentinel = other.path.join("unrelated.txt");
    std::fs::write(&sentinel, b"unrelated").unwrap();
    let (writer, browser_reader) = pipe_pair(false, &security).unwrap();
    let (reader, browser_writer) = pipe_pair(true, &security).unwrap();
    let program = std::env::current_exe().unwrap();
    let args = fixture_arguments();
    let cancel = AtomicBool::new(false);
    let (owned, control) = bootstrap::connect(
        &program,
        &args,
        &first.path,
        &security,
        &[("TERMCRAFT_PIPE_TEST_CHILD", "1".into())],
        &cancel,
    )
    .unwrap();
    bootstrap::send(
        &owned,
        &control,
        &browser_reader,
        &browser_writer,
        "",
        &cancel,
    )
    .unwrap();
    for endpoint in [&browser_reader, &browser_writer] {
        let mut flags = 0;
        assert_ne!(
            unsafe { GetHandleInformation(endpoint.as_raw_handle(), &mut flags) },
            0
        );
        assert_eq!(flags & HANDLE_FLAG_INHERIT, 0);
    }
    // Leave both app endpoints live until ordinary std Command has launched
    // the unrelated sibling, mirroring the private handle preparation window.
    let outside = std::process::Command::new(&program)
        .args(&args)
        .env("TERMCRAFT_PIPE_TEST_CHILD", "2")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let outside = OutsideProcess(outside);
    drop(browser_reader);
    drop(browser_writer);
    drop(control);
    let mut bytes = [0; 4];
    assert_eq!(
        reader
            .transfer(
                IoBuffer::Read(&mut bytes),
                Instant::now() + Duration::from_secs(5),
                &cancel
            )
            .unwrap(),
        Some(4)
    );
    let descendant = unsafe {
        handle(OpenProcess(
            PROCESS_SYNCHRONIZE,
            0,
            u32::from_le_bytes(bytes),
        ))
        .unwrap()
    };
    let marker = owned.process.as_raw_handle();
    assert_eq!(unsafe { WaitForSingleObject(marker, 0) }, WAIT_TIMEOUT);
    assert_eq!(
        unsafe { WaitForSingleObject(descendant.as_raw_handle(), 0) },
        WAIT_TIMEOUT
    );
    assert_eq!(
        writer
            .transfer(
                IoBuffer::Write(b"Q"),
                Instant::now() + Duration::from_secs(2),
                &cancel,
            )
            .unwrap(),
        Some(1)
    );
    let mut response = [0u8];
    assert_eq!(
        reader
            .transfer(
                IoBuffer::Read(&mut response),
                Instant::now() + Duration::from_secs(2),
                &cancel,
            )
            .unwrap(),
        Some(1)
    );
    assert_eq!(response, *b"Q");
    assert!(owned.stop());
    assert_eq!(unsafe { WaitForSingleObject(marker, 0) }, WAIT_OBJECT_0);
    assert_eq!(
        unsafe { WaitForSingleObject(descendant.as_raw_handle(), 0) },
        WAIT_OBJECT_0
    );
    assert_eq!(
        unsafe { WaitForSingleObject(outside.0.as_raw_handle(), 0) },
        WAIT_TIMEOUT
    );
    assert_eq!(
        reader
            .transfer(
                IoBuffer::Read(&mut response),
                Instant::now() + Duration::from_secs(2),
                &cancel,
            )
            .unwrap(),
        Some(0),
        "unrelated sibling kept the private writer alive"
    );
    drop(writer);
    drop(reader);
    drop(first);
    assert!(!owned_path.exists());
    assert_eq!(std::fs::read(sentinel).unwrap(), b"unrelated");
}

struct OutsideProcess(std::process::Child);

impl Drop for OutsideProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn fixture_arguments() -> [OsString; 2] {
    let path = module_path!().split_once("::").unwrap().1;
    [
        "--exact".into(),
        format!("{path}::windows_pipe_fixture_child").into(),
    ]
}

#[test]
fn windows_pipe_fixture_child() {
    fn write_bytes(handle: HANDLE, bytes: &[u8]) {
        let mut written = 0;
        assert_ne!(
            unsafe {
                WriteFile(
                    handle,
                    bytes.as_ptr(),
                    bytes.len() as u32,
                    &mut written,
                    null_mut(),
                )
            },
            0
        );
        assert_eq!(written as usize, bytes.len());
    }

    match std::env::var("TERMCRAFT_PIPE_TEST_CHILD").as_deref() {
        Ok("1") => {
            let (read, write, _) = bootstrap::receive().unwrap();
            let program = std::env::current_exe().unwrap();
            let cwd = std::env::current_dir().unwrap();
            let mut env = environment(&cwd).unwrap();
            let raw_read = read.as_raw_handle();
            let raw_write = write.as_raw_handle();
            env.extend([
                ("TERMCRAFT_PIPE_TEST_CHILD", "3".into()),
                (
                    "TERMCRAFT_PIPE_TEST_READ",
                    (raw_read as usize).to_string().into(),
                ),
                (
                    "TERMCRAFT_PIPE_TEST_WRITE",
                    (raw_write as usize).to_string().into(),
                ),
            ]);
            let stdio = open_null().unwrap();
            let raw_stdio = stdio.as_raw_handle();
            let descendant = spawn(
                &program,
                &fixture_arguments(),
                &env,
                &cwd,
                None,
                Some((raw_stdio, &[raw_read, raw_write, raw_stdio])),
            )
            .unwrap();
            let pid = unsafe { GetProcessId(descendant.as_raw_handle()) };
            write_bytes(raw_write, &pid.to_le_bytes());
            drop(read);
            drop(write);
            std::thread::sleep(Duration::from_secs(15));
        }
        Ok("2") => std::thread::sleep(Duration::from_secs(15)),
        Ok("3") => {
            let handle_from_env =
                |name| std::env::var(name).unwrap().parse::<usize>().unwrap() as HANDLE;
            let mut byte = [0u8];
            let mut count = 0;
            assert_ne!(
                unsafe {
                    ReadFile(
                        handle_from_env("TERMCRAFT_PIPE_TEST_READ"),
                        byte.as_mut_ptr(),
                        1,
                        &mut count,
                        null_mut(),
                    )
                },
                0
            );
            assert_eq!(count, 1);
            write_bytes(handle_from_env("TERMCRAFT_PIPE_TEST_WRITE"), &byte);
            std::thread::sleep(Duration::from_secs(15));
        }
        _ => {}
    }
}

#[test]
fn quotes_windows_arguments_with_unicode_and_trailing_backslashes() {
    let mut command = Vec::new();
    append_quoted(
        &mut command,
        OsStr::new(r#"C:\Browser tools\لغة "quoted"\"#),
    )
    .unwrap();
    let expected = r#""C:\Browser tools\لغة \"quoted\"\\""#;
    let parsed = String::from_utf16(&command).unwrap();
    assert_eq!(parsed, expected);
}

#[test]
fn browser_profile_has_a_protected_private_acl() {
    let owner = OwnerSecurity::new().unwrap();
    let profile = Profile::new(&owner).unwrap();
    let name = wide(profile.path.as_os_str()).unwrap();
    unsafe {
        let mut acl = null_mut();
        let mut descriptor = null_mut();
        assert_eq!(
            GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                &mut acl,
                null_mut(),
                &mut descriptor
            ),
            ERROR_SUCCESS
        );
        let descriptor = OwnerSecurity(descriptor);
        let (mut control, mut revision) = (0, 0);
        assert_ne!(
            GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision),
            0
        );
        assert_ne!(control & SE_DACL_PROTECTED, 0);
        assert!(!acl.is_null());
        assert_eq!((*acl).AceCount, 1);
        let mut actual = null_mut();
        assert_ne!(GetAce(acl, 0, &mut actual), 0);
        let mut expected_acl = null_mut();
        let (mut present, mut defaulted) = (0, 0);
        assert_ne!(
            GetSecurityDescriptorDacl(owner.0, &mut present, &mut expected_acl, &mut defaulted),
            0
        );
        assert_ne!(present, 0);
        let mut expected = null_mut();
        assert_ne!(GetAce(expected_acl, 0, &mut expected), 0);
        let ace = &*actual.cast::<ACCESS_ALLOWED_ACE>();
        let expected = &*expected.cast::<ACCESS_ALLOWED_ACE>();
        assert_eq!(ace.Header.AceType, expected.Header.AceType);
        assert_eq!(ace.Header.AceFlags, expected.Header.AceFlags);
        assert_eq!(ace.Mask, expected.Mask);
        assert_ne!(
            EqualSid(
                (&raw const ace.SidStart).cast_mut().cast(),
                (&raw const expected.SidStart).cast_mut().cast()
            ),
            0
        );
    }
}
