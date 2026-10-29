#import "CWBridge.h"

#import <Security/Security.h>
#import <sys/xattr.h>

static NSString *const kLemonAppPath = @"/Applications/Tencent Lemon.app";
static NSString *const kLemonBundleIdentifier = @"com.tencent.Lemon";
static NSString *const kSeedWriteMarker = @".cleanwizard-dump";

@implementation CWDumpResult

- (instancetype)initWithStatus:(CWDumpStatus)status
                   writtenPaths:(NSArray<NSString *> *)writtenPaths
          knowledgeDirectories:(NSArray<NSString *> *)knowledgeDirectories
                 ruleFileCount:(NSInteger)ruleFileCount {
    self = [super init];
    if (self) {
        _status = status;
        _writtenPaths = [writtenPaths copy];
        _knowledgeDirectories = [knowledgeDirectories copy];
        _ruleFileCount = ruleFileCount;
    }
    return self;
}

@end

@implementation CWBridge

+ (nullable NSURL *)lemonApplicationURL {
    NSFileManager *fm = [NSFileManager defaultManager];
    for (NSString *path in @[kLemonAppPath, @"/Applications/Tencent Lemon Beta.app"]) {
        BOOL isDirectory = NO;
        if ([fm fileExistsAtPath:path isDirectory:&isDirectory] && isDirectory) {
            return [NSURL fileURLWithPath:path];
        }
    }
    return nil;
}

+ (BOOL)isLemonInstalled {
    return [self lemonApplicationURL] != nil;
}

+ (nullable NSString *)lemonBinaryPath {
    NSURL *appURL = [self lemonApplicationURL];
    if (!appURL) return nil;
    NSString *executable = [NSBundle bundleWithURL:appURL].executableURL.path;
    if (executable.length > 0) return executable;
    NSFileManager *fm = [NSFileManager defaultManager];
    NSURL *macOSDirectory = [appURL URLByAppendingPathComponent:@"Contents/MacOS" isDirectory:YES];
    NSArray<NSURL *> *entries = [fm contentsOfDirectoryAtURL:macOSDirectory
                                     includingPropertiesForKeys:nil
                                                        options:NSDirectoryEnumerationSkipsHiddenFiles
                                                          error:NULL];
    for (NSURL *entry in entries) {
        if ([fm fileExistsAtPath:entry.path isDirectory:NULL]) continue;
        if ([[self architectureAtPath:entry.path] length] > 0) return entry.path;
    }
    return nil;
}

+ (NSArray<NSURL *> *)knowledgeDirectories {
    NSString *home = NSHomeDirectory();
    NSString *support = [home stringByAppendingPathComponent:@"Library/Application Support/com.tencent.lemon"];
    NSArray<NSString *> *roots = @[
        support,
        [support stringByAppendingPathComponent:@"Knowledge"],
        [support stringByAppendingPathComponent:@"KnowledgeEn"],
        [support stringByAppendingPathComponent:@"KnowledgeCh"],
    ];
    NSMutableOrderedSet<NSURL *> *found = [NSMutableOrderedSet orderedSet];
    NSFileManager *fm = [NSFileManager defaultManager];
    for (NSString *root in roots) {
        BOOL isDirectory = NO;
        if (![fm fileExistsAtPath:root isDirectory:&isDirectory] || !isDirectory) continue;
        NSArray<NSString *> *entries = [fm contentsOfDirectoryAtPath:root error:NULL] ?: @[];
        BOOL rootHoldsRules = NO;
        for (NSString *entry in entries) {
            if ([entry.pathExtension isEqualToString:@"plist"]) {
                rootHoldsRules = YES;
                break;
            }
        }
        if (rootHoldsRules) {
            [found addObject:[NSURL fileURLWithPath:root]];
            continue;
        }
        for (NSString *entry in entries) {
            if (![entry isEqualToString:@"2"] && ![entry hasSuffix:@"2"]) continue;
            NSURL *nested = [NSURL fileURLWithPath:[root stringByAppendingPathComponent:entry] isDirectory:YES];
            BOOL nestedDirectory = NO;
            if ([fm fileExistsAtPath:nested.path isDirectory:&nestedDirectory] && nestedDirectory) {
                [found addObject:nested];
            }
        }
    }
    return found.array;
}

+ (nullable NSString *)lemonKnowledgeDirectoriesDescription {
    NSArray<NSURL *> *directories = [self knowledgeDirectories];
    if (directories.count == 0) return nil;
    NSFileManager *fm = [NSFileManager defaultManager];
    NSMutableArray<NSString *> *lines = [NSMutableArray array];
    for (NSURL *directory in directories) {
        NSUInteger count = ([fm contentsOfDirectoryAtPath:directory.path error:NULL] ?: @[]).count;
        [lines addObject:[NSString stringWithFormat:@"%@ (%lu)", directory.path.stringByAbbreviatingWithTildeInPath, (unsigned long)count]];
    }
    return [lines componentsJoinedByString:@"\n"];
}

+ (NSArray<NSString *> *)binaryLiteralsMatchingTokensInFile:(NSString *)path {
    NSData *data = [NSData dataWithContentsOfFile:path options:NSDataReadingMappedIfSafe error:NULL];
    if (data.length == 0) return @[];
    const char *bytes = (const char *)data.bytes;
    NSUInteger length = data.length;
    if (length >= 4) {
        uint32_t fileMagic = ((uint32_t)bytes[0] << 24) | ((uint32_t)bytes[1] << 16) | ((uint32_t)bytes[2] << 8) | (uint32_t)bytes[3];
        const uint32_t kMachOMagic32 = 0xfeedface;
        const uint32_t kMachOMagic64 = 0xfeedfacf;
        const uint32_t kFatMagic = 0xcafebabe;
        const uint32_t kFatMagic64 = 0xcafebabf;
        BOOL isMachO = fileMagic == kMachOMagic32 || fileMagic == kMachOMagic64 ||
                       fileMagic == kFatMagic || fileMagic == kFatMagic64;
        if (!isMachO) return @[];
    }

    NSArray<NSString *> *seeds = @[
        @"Knowledge", @"KnowledgeEn", @"KnowledgeCh", @"com.tencent.lemon", @"LemonClean.db",
        @"cleanconfig", @"skipDownloadWithLocalVersion", @"Library/Application Support",
        @"Library/Caches", @"Library/Containers", @"Library/Preferences",
    ];
    NSMutableSet<NSString *> *tokens = [NSMutableSet setWithArray:seeds];
    NSMutableOrderedSet<NSString *> *matches = [NSMutableOrderedSet orderedSet];

    NSUInteger index = 0;
    while (index < length) {
        unsigned char byte = (unsigned char)bytes[index];
        if (byte < 0x20 || byte > 0x7e) {
            index++;
            continue;
        }
        NSUInteger start = index;
        while (index < length && (unsigned char)bytes[index] >= 0x20 && (unsigned char)bytes[index] <= 0x7e) {
            index++;
        }
        NSUInteger runLength = index - start;
        if (runLength < 6 || runLength > 512) continue;
        NSString *literal = [[NSString alloc] initWithBytes:bytes + start length:runLength encoding:NSUTF8StringEncoding];
        if (literal.length == 0) continue;
        for (NSString *token in tokens) {
            if ([literal containsString:token]) {
                [matches addObject:literal];
                break;
            }
        }
    }
    return matches.array;
}

+ (CWDumpResult *)dumpLocalLemonRulesToDirectory:(NSURL *)destination {
    NSFileManager *fm = [NSFileManager defaultManager];
    [fm createDirectoryAtURL:destination withIntermediateDirectories:YES attributes:nil error:NULL];

    BOOL installed = [self isLemonInstalled];
    NSURL *appURL = [self lemonApplicationURL];
    NSArray<NSURL *> *knowledge = [self knowledgeDirectories];

    NSMutableArray<NSString *> *written = [NSMutableArray array];
    NSMutableArray<NSString *> *knowledgePaths = [NSMutableArray array];
    for (NSURL *directory in knowledge) {
        [knowledgePaths addObject:directory.path];
    }

    NSString *binaryPath = [self lemonBinaryPath];
    NSDictionary *manifest = @{
        @"generator": @"CleanWizard local lemon dump",
        @"bundleIdentifier": kLemonBundleIdentifier,
        @"applicationPath": appURL.path ?: @"",
        @"installed": @(installed),
        @"binaryPath": binaryPath ?: @"",
        @"knowledgeDirectories": knowledgePaths,
        @"marker": kSeedWriteMarker,
        @"networkAccess": @"none",
    };
    NSURL *manifestURL = [destination URLByAppendingPathComponent:@"lemon-dump-manifest.json"];
    NSData *manifestData = [NSJSONSerialization dataWithJSONObject:manifest options:NSJSONWritingPrettyPrinted | NSJSONWritingSortedKeys error:NULL];
    if (manifestData && [manifestData writeToURL:manifestURL atomically:YES]) {
        [written addObject:manifestURL.lastPathComponent];
    }

    NSUInteger ruleCount = 0;
    NSInteger directoryIndex = 0;
    for (NSURL *directory in knowledge) {
        NSString *tag = [NSString stringWithFormat:@"%@%ld", directory.path.lastPathComponent, (long)directoryIndex];
        directoryIndex += 1;
        NSArray<NSString *> *entries = [fm contentsOfDirectoryAtPath:directory.path error:NULL] ?: @[];
        for (NSString *entry in entries) {
            if (![entry.pathExtension isEqualToString:@"plist"]) continue;
            NSURL *source = [directory URLByAppendingPathComponent:entry];
            NSString *targetName = [NSString stringWithFormat:@"lemon-%@-%@", tag, entry];
            NSURL *target = [destination URLByAppendingPathComponent:targetName];
            if ([fm copyItemAtURL:source toURL:target error:NULL]) {
                [written addObject:targetName];
                ruleCount++;
            }
        }
    }

    NSArray<NSString *> *literals = binaryPath ? [self binaryLiteralsMatchingTokensInFile:binaryPath] : @[];
    NSDictionary *literalPayload = @{
        @"binaryPath": binaryPath ?: @"",
        @"literals": literals,
    };
    NSData *literalData = [NSJSONSerialization dataWithJSONObject:literalPayload options:NSJSONWritingPrettyPrinted | NSJSONWritingSortedKeys error:NULL];
    if (literalData) {
        NSURL *literalURL = [destination URLByAppendingPathComponent:@"lemon-binary-literals.json"];
        if ([literalData writeToURL:literalURL atomically:YES]) {
            [written addObject:literalURL.lastPathComponent];
        }
    }

    CWDumpStatus status = CWDumpStatusSuccess;
    if (!installed) {
        status = CWDumpStatusLemonNotFound;
    } else if (knowledge.count == 0) {
        status = CWDumpStatusNoKnowledgeRules;
    }

    return [[CWDumpResult alloc] initWithStatus:status
                                   writtenPaths:written
                           knowledgeDirectories:knowledgePaths
                                  ruleFileCount:ruleCount];
}

+ (BOOL)isCodeSignedAtPath:(NSString *)path {
    NSURL *url = [NSURL fileURLWithPath:path];
    SecStaticCodeRef code = NULL;
    OSStatus status = SecStaticCodeCreateWithPath((__bridge CFURLRef)url, kSecCSDefaultFlags, &code);
    if (status != errSecSuccess || code == NULL) return NO;
    status = SecStaticCodeCheckValidity(code, kSecCSDoNotValidateResources | kSecCSCheckAllArchitectures, NULL);
    CFRelease(code);
    return status == errSecSuccess;
}

+ (nullable NSString *)architectureAtPath:(NSString *)path {
    NSData *header = [NSData dataWithContentsOfFile:path options:NSDataReadingMappedIfSafe error:NULL];
    if (header.length < 8) return nil;
    const uint8_t *bytes = (const uint8_t *)header.bytes;
    uint32_t fileMagic = ((uint32_t)bytes[0] << 24) | ((uint32_t)bytes[1] << 16) | ((uint32_t)bytes[2] << 8) | (uint32_t)bytes[3];
    const uint32_t kMachOMagic64 = 0xfeedfacf;
    const uint32_t kMachOMagic32 = 0xfeedface;
    const uint32_t kFatMagic = 0xcafebabe;
    if (fileMagic == kFatMagic) return @"Universal";
    if (fileMagic != kMachOMagic64 && fileMagic != kMachOMagic32) return nil;
    uint32_t cpuType = ((uint32_t)bytes[4] << 24) | ((uint32_t)bytes[5] << 16) | ((uint32_t)bytes[6] << 8) | (uint32_t)bytes[7];
    const uint32_t kCPUArchABI64 = 0x01000000;
    const uint32_t kCPUTypeARM64 = 0x0100000c;
    const uint32_t kCPUTypeX86_64 = 0x01000007;
    uint32_t baseType = cpuType & ~kCPUArchABI64;
    if (baseType == kCPUTypeARM64) return @"arm64";
    if (baseType == kCPUTypeX86_64) return @"x86_64";
    return @"unknown";
}

+ (NSDictionary<NSString *, id> *)signatureInfoAtPath:(NSString *)path {
    NSDictionary *info = @{
        @"signed": @([self isCodeSignedAtPath:path]),
        @"architecture": [self architectureAtPath:path] ?: @"unknown",
    };
    return info;
}

+ (NSDictionary<NSString *, id> *)extendedAttributeSummaryAtPath:(NSString *)path {
    NSMutableDictionary<NSString *, id> *summary = [NSMutableDictionary dictionary];
    const char *cPath = [path fileSystemRepresentation];
    if (cPath == NULL) return summary;
    ssize_t listSize = listxattr(cPath, NULL, 0, XATTR_NOFOLLOW);
    if (listSize <= 0) return summary;
    char *names = calloc((size_t)listSize, sizeof(char));
    if (names == NULL) return summary;
    ssize_t readSize = listxattr(cPath, names, (size_t)listSize, XATTR_NOFOLLOW);
    if (readSize > 0) {
        for (ssize_t offset = 0; offset < readSize; offset += (ssize_t)strlen(names + offset) + 1) {
            NSString *name = [NSString stringWithUTF8String:names + offset];
            if (name.length > 0) {
                size_t valueSize = getxattr(cPath, names + offset, NULL, 0, 0, XATTR_NOFOLLOW);
                summary[name] = valueSize > 0 ? [NSString stringWithFormat:@"%zu bytes", valueSize] : @"empty";
            }
        }
    }
    free(names);
    return summary;
}

@end
