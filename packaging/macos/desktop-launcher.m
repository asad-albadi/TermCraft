// AppKit receives Finder/URL events; winit/eframe does not turn them into argv.
// Keep the Rust executable and helper adjacent, and dispatch using NSTask arguments.
#import <AppKit/AppKit.h>
@interface TermCraftLaunchDelegate : NSObject <NSApplicationDelegate>
@property(strong) NSTask *primary;
@property BOOL receivedOpen;
@property(strong) NSArray<NSString *> *workspaceArguments;
@end
@implementation TermCraftLaunchDelegate
- (void)launchArguments:(NSArray<NSString *> *)arguments {
    NSString *program = [[[NSBundle mainBundle] executablePath].stringByDeletingLastPathComponent stringByAppendingPathComponent:@"harbor-workspace"];
    NSTask *task = [NSTask new];
    task.executableURL = [NSURL fileURLWithPath:program];
    task.arguments = arguments;
    BOOL ownsLifetime = self.primary == nil;
    if (ownsLifetime) {
        self.primary = task;
        task.terminationHandler = ^(NSTask *finished) {
            dispatch_async(dispatch_get_main_queue(), ^{ [NSApp terminate:nil]; });
        };
    }
    NSError *error = nil;
    if (![task launchAndReturnError:&error]) {
        NSAlert *alert = [NSAlert new];
        alert.messageText = @"TermCraft could not start";
        alert.informativeText = error.localizedDescription;
        [alert runModal];
        if (ownsLifetime) [NSApp terminate:nil];
    }
}
- (void)applicationDidFinishLaunching:(NSNotification *)notification {
    if (!self.receivedOpen) {
        NSArray *arguments = [NSProcessInfo processInfo].arguments;
        // Native command-line launches preserve explicit --data-dir and normal CLI actions.
        [self launchArguments: arguments.count > 1 ? [arguments subarrayWithRange:NSMakeRange(1, arguments.count - 1)] : @[]];
    }
}
- (void)application:(NSApplication *)application openURLs:(NSArray<NSURL *> *)urls {
    self.receivedOpen = YES;
    if (urls.count > 64) return;
    for (NSURL *url in urls) {
        if (url.isFileURL) {
            NSNumber *isDirectory = nil;
            [url getResourceValue:&isDirectory forKey:NSURLIsDirectoryKey error:nil];
            [self launchArguments:[self.workspaceArguments arrayByAddingObjectsFromArray:@[isDirectory.boolValue ? @"--cwd" : @"--open-file", url.path]]];
        } else {
            [self launchArguments:[self.workspaceArguments arrayByAddingObjectsFromArray:@[@"--open-url", url.absoluteString]]];
        }
    }
}
- (void)openTerminalHere:(NSPasteboard *)pasteboard userData:(NSString *)userData error:(NSString **)error {
    NSArray<NSURL *> *urls = [pasteboard readObjectsForClasses:@[[NSURL class]] options:@{NSPasteboardURLReadingFileURLsOnlyKey:@YES}];
    if (urls.count == 0 || urls.count > 64) {
        if (error) *error = @"Select a folder in Finder.";
        return;
    }
    for (NSURL *url in urls) {
        NSNumber *isDirectory = nil;
        [url getResourceValue:&isDirectory forKey:NSURLIsDirectoryKey error:nil];
        if (!url.isFileURL || !isDirectory.boolValue) {
            if (error) *error = @"Open terminal here requires a local folder.";
            return;
        }
    }
    self.receivedOpen = YES;
    for (NSURL *url in urls) [self launchArguments:[self.workspaceArguments arrayByAddingObjectsFromArray:@[@"--cwd",url.path]]];
}
- (BOOL)applicationShouldHandleReopen:(NSApplication *)application hasVisibleWindows:(BOOL)visible {
    [self launchArguments:self.workspaceArguments];
    return NO;
}
@end
int main(int argc, const char *argv[]) {
    @autoreleasepool {
        NSApplication *application = [NSApplication sharedApplication];
        [application setActivationPolicy:NSApplicationActivationPolicyAccessory];
        TermCraftLaunchDelegate *delegate = [TermCraftLaunchDelegate new];
        NSMutableArray *workspace = [NSMutableArray new];
        NSArray *arguments = [NSProcessInfo processInfo].arguments;
        for (NSUInteger i=1; i+1<arguments.count; i++) {
            if ([arguments[i] isEqualToString:@"--data-dir"]) {
                [workspace addObject:arguments[i]]; [workspace addObject:arguments[i+1]]; break;
            }
        }
        delegate.workspaceArguments = workspace;
        application.delegate = delegate;
        application.servicesProvider = delegate;
        [application run];
    }
    return 0;
}
