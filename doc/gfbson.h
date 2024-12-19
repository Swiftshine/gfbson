// Note: I did no code RE. This is done with a hex editor and deductions.
// Whether it's truly accurate remains to be seen.

// This is a very incomplete documentation.
#include <cstdint>

/* Flags */
enum EntityFlags {
    // This entity keeps track of a size of some sort.
    SizeTracking = 0x1,

    // This entity indicates the start of the BSON file.
    BSONStart = 0x2,

    // A common starting entity flag.
    // It keeps track of the BSON file size.
    BSONStartWithFilesize = SizeTracking | BSONStart,

    Unknown4 = 0x4,

    // This entity has some kind of value.
    EntityWithValue = 0x8,

    Unknown10 = 0x10,
    Unknown20 = 0x20,

    // This entity is a container (i.e. an object, an array...).
    Container = 0x40,

    // A template for a JSON object.
    Object = EntityWithValue | Unknown10 | Container,

    Unknown80 = 0x80,

    // A template for a JSON array.
    Array = EntityWithValue | Container | Unknown80,

    Unknown100 = 0x100,
};

// Note: Entities with a flag of 0xE0 are 0x8 bytes long instead of 0x10

/* Types */

// The type of the next entity in an object.
enum NextEntityType : uint32_t {
    /// @brief The next entity should be interpreted as a signed 4-byte integer.
    Int = 0x12F,

    /// @brief The next entity should be interpreted as a UTF-8 string.
    String = 0x131,


    /// @brief The next entity should be interpreted as an object.
    Object = 0x12D,

    /// @brief The next entry should be interpreted as an array.
    Array = 0x12E,
};

/* Object Entries */
struct EntryBase {
    uint32_t mFlags;
};

// 0x2
struct StartFileEntry : EntryBase {
    uint32_t m_4;
    uint32_t mFilesize;
    NextEntityType mNextType;
};

// 0x8
struct IntEntry : EntryBase {
    uint32_t mLabelIndex;
    int32_t mValue;
    NextEntityType mNextType;
};

// 0x8
struct StringEntry : EntryBase {
    uint32_t mLabelIndex;
    uint32_t mValueIndex;
    NextEntityType mNextType;
};

// 0xC8
struct ArrayEntry : EntryBase {
    uint32_t mLabelIndex;
    uint32_t mNumEntries;
    NextEntityType mNextType;
};


// 0x58
struct ObjectEntry : EntryBase {
    uint32_t m_4;
    uint32_t mNumEntries;
    NextEntityType mNextType;
};
