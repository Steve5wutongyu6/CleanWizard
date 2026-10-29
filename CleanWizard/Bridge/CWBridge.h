#import <Foundation/Foundation.h>

NS_ASSUME_NONNULL_BEGIN

typedef NS_ENUM(NSInteger, CWDumpStatus) {
    CWDumpStatusSuccess = 0,
    CWDumpStatusLemonNotFound = 1,
    CWDumpStatusNoKnowledgeRules = 2,
};

@interface CWDumpResult : NSObject

@property (nonatomic, readonly) CWDumpStatus status;
@property (nonatomic, copy, readonly) NSArray<NSString *> *writtenPaths;
@property (nonatomic, copy, readonly) NSArray<NSString *> *knowledgeDirectories;
@property (nonatomic, readonly) NSInteger ruleFileCount;

@end

@interface CWBridge : NSObject

+ (BOOL)isLemonInstalled;
+ (nullable NSURL *)lemonApplicationURL;
+ (nullable NSString *)lemonBinaryPath;
+ (NSArray<NSURL *> *)knowledgeDirectories;
+ (nullable NSString *)lemonKnowledgeDirectoriesDescription;

+ (CWDumpResult *)dumpLocalLemonRulesToDirectory:(NSURL *)destination;

+ (BOOL)isCodeSignedAtPath:(NSString *)path;
+ (nullable NSString *)architectureAtPath:(NSString *)path;
+ (NSDictionary<NSString *, id> *)signatureInfoAtPath:(NSString *)path;
+ (NSDictionary<NSString *, id> *)extendedAttributeSummaryAtPath:(NSString *)path;

@end

NS_ASSUME_NONNULL_END
