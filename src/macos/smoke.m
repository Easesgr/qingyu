// Standalone AppKit integration smoke test. Windows stay off-screen or fully
// transparent while checking a real screen's top-edge frame constraint.
// Run from the repository root (no Rust build required):
// xcrun clang -fobjc-arc -fblocks -mmacosx-version-min=12.0 src/macos/smoke.m src/macos/notifications.m -framework AppKit -framework ApplicationServices -framework Foundation -framework CoreGraphics -framework CoreAudio -framework IOKit -framework SystemConfiguration -framework QuartzCore -framework UniformTypeIdentifiers -o /tmp/qingyu-native-smoke
// /tmp/qingyu-native-smoke
#import "native.m"
#include <assert.h>
static unsigned wakes=0;
static void wake_test(void *context) {wakes++;}
static void top_strip_geometry_smoke(void) {
    QYRect display={-1920,-500,1920,1080},window={-1880,-560,800,500};
    for(double scale=0.8;scale<=2.01;scale+=0.2) {
        QYRect top={20/scale,60/scale,100/scale,30/scale};
        assert(qy_overlaps_menu_bar(top,window,scale,display,-476));
        QYRect below=top;below.y+=25/scale;
        assert(!qy_overlaps_menu_bar(below,window,scale,display,-476));
        QYRect empty={0};assert(!qy_overlaps_menu_bar(empty,window,scale,display,-476));
    }
}
// Inject only button state and drag-board metadata. Do not synthesize desktop
// events or read/write any pasteboard payload while verifying this lifecycle.
static void file_drag_state_smoke(void) {
    assert(qy_drag_types_contain_files(@[NSPasteboardTypeFileURL]));
    assert(qy_drag_types_contain_files(@[@"NSFilenamesPboardType"]));
    assert(!qy_drag_types_contain_files(@[NSPasteboardTypeString,NSPasteboardTypeURL]));
    assert(!qy_drag_types_contain_files(nil));
    QYFileDragState state={0};
    QYRect hit={0,0,6,52},notice={0},drop={0,0,419,236};
    NSPoint background=NSMakePoint(200,120);
    // A regular background-window/scrollbar drag does not own the file zone.
    BOOL files=qy_file_drag_step(&state,YES,(QYDragObservation){40,NO});
    assert(!files && !qy_mouse_decision(background,YES,files,NO,NO,hit,notice,drop).accept);
    files=qy_file_drag_step(&state,YES,(QYDragObservation){40,NO});assert(!files);
    // A stale previous file payload cannot activate just because a button is held.
    files=qy_file_drag_step(&state,YES,(QYDragObservation){40,YES});
    assert(!files && !qy_mouse_decision(background,YES,files,NO,NO,hit,notice,drop).accept);
    // Nor does a fresh text/URL drag qualify. A source may populate types after
    // declaring its board, so the next observation at the same count can qualify.
    assert(!qy_file_drag_step(&state,YES,(QYDragObservation){41,NO}));
    files=qy_file_drag_step(&state,YES,(QYDragObservation){41,YES});assert(files);
    assert(qy_mouse_decision(background,YES,files,NO,NO,hit,notice,drop).accept);
    assert(!qy_mouse_decision(NSMakePoint(500,120),YES,files,NO,NO,hit,notice,drop).accept);
    assert(!qy_mouse_decision(background,YES,files,NO,YES,hit,notice,drop).accept);
    assert(qy_file_drag_step(&state,YES,(QYDragObservation){41,YES}));
    // Release completes the session. Unchanged file types on the next press
    // stay stale, even if no idle timer tick occurred between the two gestures.
    assert(!qy_file_drag_step(&state,NO,(QYDragObservation){41,YES}));
    assert(!qy_file_drag_step(&state,YES,(QYDragObservation){41,YES}));
    assert(!qy_file_drag_step(&state,YES,(QYDragObservation){41,YES}));
    assert(qy_file_drag_step(&state,YES,(QYDragObservation){42,YES}));
    assert(!qy_file_drag_step(&state,NO,(QYDragObservation){42,YES}));
    // Startup while already held is conservative: an existing payload is not
    // evidence that this app observed a new drag during the current gesture.
    QYFileDragState startup={0};assert(!qy_file_drag_step(&startup,YES,(QYDragObservation){99,YES}));
    // Local island drags keep capture outside every region, until primary release.
    QYMouseDecision captured=qy_mouse_decision(NSMakePoint(600,400),YES,NO,YES,NO,hit,notice,drop);
    assert(captured.accept && captured.captured);
    QYMouseDecision released=qy_mouse_decision(NSMakePoint(600,400),NO,NO,captured.captured,NO,hit,notice,drop);
    assert(!released.accept && !released.captured);
    assert(!qy_mouse_decision(background,NO,YES,NO,NO,hit,notice,drop).accept);
    assert(qy_mouse_decision(NSMakePoint(3,26),NO,NO,NO,NO,hit,notice,drop).accept);
}
static void file_drag_window_smoke(QYShell *shell) {
    shell.hit=(QYRect){0,0,6,52};shell.notice=(QYRect){0};shell.dropZone=(QYRect){0,0,419,236};shell.captured=NO;
    NSPoint point=NSMakePoint(200,120);CGFloat alpha=shell.window.alphaValue;
    [shell updateMouseAt:point primaryDown:NO drag:(QYDragObservation){100,YES}];
    assert(shell.window.ignoresMouseEvents);
    [shell updateMouseAt:point primaryDown:YES drag:(QYDragObservation){100,YES}];
    assert(shell.window.ignoresMouseEvents && !shell.fileDropActive);
    [shell updateMouseAt:point primaryDown:YES drag:(QYDragObservation){100,YES}];
    assert(shell.window.ignoresMouseEvents);
    unsigned before=wakes;
    [shell updateMouseAt:point primaryDown:YES drag:(QYDragObservation){101,YES}];
    assert(!shell.window.ignoresMouseEvents && shell.fileDropActive && wakes==before+1);
    [shell updateMouseAt:point primaryDown:YES drag:(QYDragObservation){101,YES}];
    assert(!shell.window.ignoresMouseEvents);
    [shell updateMouseAt:point primaryDown:NO drag:(QYDragObservation){101,YES}];
    assert(shell.window.ignoresMouseEvents && !shell.fileDropActive);
    [shell updateMouseAt:point primaryDown:YES drag:(QYDragObservation){101,YES}];
    assert(shell.window.ignoresMouseEvents);
    shell.captured=YES;
    [shell updateMouseAt:NSMakePoint(600,400) primaryDown:YES drag:(QYDragObservation){101,YES}];
    assert(!shell.window.ignoresMouseEvents);
    [shell updateMouseAt:NSMakePoint(600,400) primaryDown:NO drag:(QYDragObservation){101,YES}];
    assert(shell.window.ignoresMouseEvents && !shell.captured);
    assert(shell.window.alphaValue==alpha && !shell.window.opaque);
    printf("File drag lifecycle passed: ordinary/stale drags pass through; fresh file drops and local capture end on release\n");
}
static void portrait_geometry_smoke(void *native,QYShell *shell) {
    for(int right=0;right<2;right++) {
        for(NSNumber *number in @[@0.6,@1.0,@1.25,@2.0]) {
            double scale=number.doubleValue,x=right?208:0;
            QYRect frame={-10000,-10000,500*scale,600*scale};
            assert(qy_shell_place(native,frame));
            QYRect body={x,34,292,532},hit={x,12,292,576},empty={0};
            double depths[50],lefts[50],rights[50];size_t rows=0;
            for(int i=0;i<=24;i++) {double a=i/24.0*M_PI_2;depths[rows]=22*(1-cos(a));lefts[rows]=34-22*(1-sin(a));rights[rows]=566+22*(1-sin(a));rows++;}
            depths[rows]=266;lefts[rows]=34;rights[rows]=566;rows++;
            for(int i=1;i<=24;i++) {double a=i/24.0*M_PI_2;depths[rows]=266+26*sin(a);lefts[rows]=34+26-26*cos(a);rights[rows]=566-26+26*cos(a);rows++;}
            double points[200];size_t at=0;
            for(size_t i=0;i<rows;i++) {points[at++]=right?500-depths[i]:depths[i];points[at++]=rights[i];}
            for(size_t i=rows;i-->0;) {points[at++]=right?500-depths[i]:depths[i];points[at++]=lefts[i];}
            qy_shell_hit_outline(native,points,at/2);
            qy_shell_update(native,hit,empty,body,scale,0,0);
            [shell updateMouseAt:NSMakePoint(x+146,300) primaryDown:NO drag:(QYDragObservation){200,NO}];
            assert(!shell.window.ignoresMouseEvents);
            [shell updateMouseAt:NSMakePoint(right?50:450,300) primaryDown:NO drag:(QYDragObservation){200,NO}];
            assert(shell.window.ignoresMouseEvents);
            [shell updateMouseAt:NSMakePoint(right?480:20,14) primaryDown:NO drag:(QYDragObservation){200,NO}];assert(shell.window.ignoresMouseEvents);
            [shell updateMouseAt:NSMakePoint(right?499.99:0.01,20) primaryDown:NO drag:(QYDragObservation){200,NO}];assert(!shell.window.ignoresMouseEvents);
            [shell updateMouseAt:NSMakePoint(right?209:291,35) primaryDown:NO drag:(QYDragObservation){200,NO}];assert(shell.window.ignoresMouseEvents);
            [shell updateMouseAt:NSMakePoint(x+146,595) primaryDown:NO drag:(QYDragObservation){200,NO}];
            assert(shell.window.ignoresMouseEvents);
            uint32_t counts[]={(uint32_t)(at/2)};
            if(qy_shell_blur(native,points,counts,1)) {
                CGRect bounds=CGPathGetBoundingBox(shell.blurMask.path);
                assert(fabs(bounds.origin.x-x*scale)<0.01);
                assert(fabs(bounds.origin.y-12*scale)<0.01);
                assert(fabs(bounds.size.width-292*scale)<0.01 && fabs(bounds.size.height-576*scale)<0.01);
                assert(shell.blurWindow.ignoresMouseEvents);
            }
        }
    }
    qy_shell_hit_outline(native,NULL,0);
    printf("Portrait native geometry passed: both edges, 0.6–2x logical scale, transparent desktop and blur mask\n");
}
static void floating_notice_geometry_smoke(void *native,QYShell *shell) {
    QYRect body={120,20,220,30},empty={0};
    for(NSNumber *value in @[@0.08,@0.3,@0.7,@1.0]) {
        double progress=value.doubleValue,height=44*progress,radius=fmin(22,height/2);
        QYRect notice={80,60,380,height};
        double points[104];size_t at=0;
        double centers[][3]={{notice.x+notice.w-radius,notice.y+radius,-M_PI_2},
            {notice.x+notice.w-radius,notice.y+notice.h-radius,0},
            {notice.x+radius,notice.y+notice.h-radius,M_PI_2},{notice.x+radius,notice.y+radius,M_PI}};
        for(int corner=0;corner<4;corner++)for(int step=0;step<=12;step++) {
            double angle=centers[corner][2]+step/12.0*M_PI_2;
            points[at++]=centers[corner][0]+radius*cos(angle);points[at++]=centers[corner][1]+radius*sin(angle);
        }
        uint32_t counts[]={52};
        qy_shell_notice_outlines(native,points,counts,1);qy_shell_update(native,body,notice,empty,1,0,0);
        [shell updateMouseAt:NSMakePoint(270,notice.y+height/2) primaryDown:NO drag:(QYDragObservation){300,NO}];assert(!shell.window.ignoresMouseEvents);
        [shell updateMouseAt:NSMakePoint(notice.x+0.01,notice.y+0.01) primaryDown:NO drag:(QYDragObservation){300,NO}];assert(shell.window.ignoresMouseEvents);
        [shell updateMouseAt:NSMakePoint(240,55) primaryDown:NO drag:(QYDragObservation){300,NO}];assert(shell.window.ignoresMouseEvents);
        [shell updateMouseAt:NSMakePoint(240,35) primaryDown:NO drag:(QYDragObservation){300,NO}];assert(!shell.window.ignoresMouseEvents);
        if(qy_shell_blur(native,points,counts,1)) {
            CGRect bounds=CGPathGetBoundingBox(shell.blurMask.path);
            assert(fabs(bounds.origin.x-notice.x)<0.01&&fabs(bounds.origin.y-(shell.window.frame.size.height-notice.y-notice.h))<0.01);
            assert(fabs(bounds.size.width-notice.w)<0.01&&fabs(bounds.size.height-notice.h)<0.01);
        }
    }
    qy_shell_notice_outlines(native,NULL,NULL,0);
    printf("Floating native row passed: animated mask, independent island, rounded-corner and gap click-through\n");
}
static void disclosure_geometry_smoke(void *native,QYShell *shell,QYScreen *screens,size_t screenCount) {
    QYRect saved=top_rect(shell.window.frame);CGFloat alpha=shell.window.alphaValue;
    // A real screen is required here: an entirely off-screen canvas does not
    // exercise AppKit's menu-bar frame constraint. Keep the test transparent.
    shell.window.alphaValue=0;
    for(size_t i=0;i<screenCount;i++)for(NSNumber *value in @[@0.8,@1.0,@1.4]) {
        double scale=value.doubleValue;
        QYRect requested={screens[i].frame.x+80,screens[i].frame.y-180*scale,640*scale,600*scale};
        assert(qy_shell_place(native,requested));
        [NSRunLoop.currentRunLoop runUntilDate:[NSDate dateWithTimeIntervalSinceNow:0.02]];
        QYRect observed;double backing;assert(qy_shell_geometry(native,&observed,&backing));
        if(fabs(requested.y-observed.y)>0.01)fprintf(stderr,"Top-edge canvas moved: requested %.2f, observed %.2f, scale %.2f\n",requested.y,observed.y,scale);
        assert(fabs(requested.x-observed.x)<0.01&&fabs(requested.y-observed.y)<0.01);
        assert(fabs(observed.y+180*scale-screens[i].frame.y)<0.01);
    }
    assert(qy_shell_place(native,saved));shell.window.alphaValue=alpha;
    double points[]={120,20,340,20,340,50,120,50,20,58,439,58,439,344,20,344};uint32_t counts[]={4,4};
    QYRect hit={20,20,419,324},empty={0};
    qy_shell_hit_outlines(native,points,counts,2);qy_shell_update(native,hit,empty,empty,1,0,0);
    for(NSNumber *value in @[@35,@100]) {
        [shell updateMouseAt:NSMakePoint(230,value.doubleValue) primaryDown:NO drag:(QYDragObservation){400,NO}];assert(!shell.window.ignoresMouseEvents);
    }
    [shell updateMouseAt:NSMakePoint(230,54) primaryDown:NO drag:(QYDragObservation){400,NO}];assert(shell.window.ignoresMouseEvents);
    [shell updateMouseAt:NSMakePoint(100,35) primaryDown:NO drag:(QYDragObservation){400,NO}];assert(shell.window.ignoresMouseEvents);
    qy_shell_hit_outlines(native,NULL,NULL,0);
    printf("Downward disclosure passed: real screen top-edge canvas roundtrip at 0.8–1.4x and disjoint native input\n");
}
int main(void) {
    @autoreleasepool {
        [NSApplication sharedApplication];
        file_drag_state_smoke();
        top_strip_geometry_smoke();
        QYScreen screens[32];size_t count=qy_screens(screens,32);assert(count>0);
        for(size_t i=0;i<count;i++) {assert(screens[i].frame.w>0 && screens[i].frame.h>0);assert(screens[i].scale>=1);assert(screens[i].identifier[0]);
            if(screens[i].camera.w>0) {
                assert(screens[i].safe_top>0 && screens[i].camera.h==screens[i].safe_top);
                assert(screens[i].camera.x>=screens[i].frame.x && screens[i].camera.x+screens[i].camera.w<=screens[i].frame.x+screens[i].frame.w);
                assert(screens[i].camera.y==screens[i].frame.y);
                printf("Camera housing: %.0f × %.0f pt, screen origin %.0f, %.0f; work top %.0f\n",screens[i].camera.w,screens[i].camera.h,screens[i].frame.x,screens[i].frame.y,screens[i].work.y);
            }
        }
        QYRect negative={-1920,-340,960,270};QYRect roundtrip=top_rect(cocoa_rect(negative));assert(memcmp(&negative,&roundtrip,sizeof(negative))==0);
        NSWindow *window=[[NSWindow alloc] initWithContentRect:NSMakeRect(-10000,-10000,960,270) styleMask:NSWindowStyleMaskBorderless backing:NSBackingStoreBuffered defer:NO];
        NSWindow *settings=[[NSWindow alloc] initWithContentRect:NSMakeRect(-10000,-10000,600,450) styleMask:NSWindowStyleMaskTitled|NSWindowStyleMaskClosable|NSWindowStyleMaskResizable backing:NSBackingStoreBuffered defer:NO];
        BOOL settingsKey=settings.canBecomeKeyWindow,settingsMain=settings.canBecomeMainWindow;
        NSRect settingsRequested=cocoa_rect((QYRect){screens[0].frame.x+40,screens[0].frame.y-180,600,450});
        NSRect settingsConstrained=[settings constrainFrameRect:settingsRequested toScreen:NSScreen.screens.firstObject];
        assert(settingsKey);
        void *native=qy_shell_create((__bridge void *)window.contentView,wake_test,NULL);assert(native);
        assert(!window.canBecomeKeyWindow);assert(window.ignoresMouseEvents);assert(!window.opaque);
        assert(settings.canBecomeKeyWindow==settingsKey && settings.canBecomeMainWindow==settingsMain);
        assert(NSEqualRects(settingsConstrained,[settings constrainFrameRect:settingsRequested toScreen:NSScreen.screens.firstObject]));
        QYRect frame={-10000,-10000,960,270};assert(qy_shell_place(native,frame));
        QYRect observed;double backing;assert(qy_shell_geometry(native,&observed,&backing));assert(memcmp(&frame,&observed,sizeof(frame))==0);assert(backing>=1);
        QYRect top={screens[0].frame.x+20-frame.x,screens[0].frame.y-frame.y,100,20},none={0};
        qy_shell_update(native,top,none,none,1,0,0);assert(window.level==NSStatusWindowLevel+1);
        qy_shell_update(native,none,none,none,1,0,0);assert(window.level==NSFloatingWindowLevel);
        double x,y;assert(qy_shell_cursor(native,&x,&y));
        QYRect hit={x-10,y-10,20,20},empty={0,0,0,0};
        qy_shell_update(native,hit,empty,empty,1,0,0);assert(!window.ignoresMouseEvents);
        qy_shell_update(native,empty,empty,empty,1,0,0);assert(window.ignoresMouseEvents);
        qy_shell_update(native,hit,empty,empty,1,0,1);assert(window.ignoresMouseEvents && window.alphaValue==0);
        qy_shell_update(native,hit,empty,empty,1,0,0);assert(window.alphaValue==1);
        QYShell *shell=(__bridge QYShell *)native;
        file_drag_window_smoke(shell);
        portrait_geometry_smoke(native,shell);
        floating_notice_geometry_smoke(native,shell);
        disclosure_geometry_smoke(native,shell,screens,count);
        qy_shell_menu(native,1,0,1);assert([shell.status.menu itemWithTag:4]==nil);assert([shell.status.menu itemWithTag:8]==nil);assert([shell.status.menu itemWithTag:16].state==NSControlStateValueOn);
        [shell event:[shell.status.menu itemWithTag:1]];assert(qy_shell_events(native)==1);assert(qy_shell_events(native)==0);
        double points[]={0,0,100,0,100,40,0,40};uint32_t paths[]={4};
        int blur=qy_shell_blur(native,points,paths,1);
        if(!NSWorkspace.sharedWorkspace.accessibilityDisplayShouldReduceTransparency) {assert(blur);assert(shell.blurWindow.parentWindow==window);assert(shell.blurWindow.ignoresMouseEvents);assert(shell.blurMask.path);}
        assert(!qy_shell_blur(native,NULL,NULL,0));assert(shell.blurWindow.alphaValue==0);
        assert(qy_drag_file(native,"/tmp/no-file")==0);
        QYSystem snapshot;qy_system_sample(&snapshot);assert(snapshot.battery>=-1 && snapshot.battery<=100);assert(snapshot.volume>=-1 && snapshot.volume<=100);assert(snapshot.idle>=0);
        // The blur child can close before Rust drops its shell (winit exit).
        // ARC ownership must keep the property valid until that final cleanup.
        if(shell.blurWindow){assert(!shell.blurWindow.releasedWhenClosed);[shell.blurWindow close];assert(shell.blurWindow.contentView);}
        NSWindow *formerBlur=shell.blurWindow;
        qy_shell_destroy(native);assert(objc_getAssociatedObject(window,QYShellKey)==nil);
        if(formerBlur)assert(!NSEqualRects(settingsRequested,[formerBlur constrainFrameRect:settingsRequested toScreen:NSScreen.screens.firstObject]));
        assert(NSEqualRects(settingsConstrained,[settings constrainFrameRect:settingsRequested toScreen:NSScreen.screens.firstObject]));
        assert(!NSEqualRects(settingsRequested,[window constrainFrameRect:settingsRequested toScreen:NSScreen.screens.firstObject]));
        [window orderOut:nil];[settings orderOut:nil];
        printf("Native smoke passed: %zu screens, backing %.0fx, battery %d%%, audio %d%%, network %d, %u wake callbacks\n",count,backing,snapshot.battery,snapshot.volume,snapshot.network,wakes);
    }
}
