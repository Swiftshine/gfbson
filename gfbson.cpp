#include <iostream>
#include "gfbson.h"

Node BSON::ReadNode() {
    uint32_t type = mCursor.Read32();
    uint32_t size = mCursor.Read32();

    switch (static_cast<NodeType>(type)) {
        case NodeType::Root: {
            RootNode node;
            node.mType = type;
            node.mContentsSize = size;
            return node;
        }

        case NodeType::Object: {
            ObjectNode node;
            node.mType = type;
            node.mContentsSize = size;
            node.Read(mCursor);
            return node;
        }

        // case NodeType::Array: {
        //     ArrayNode node;
        //     node.mType = type;
        //     node.mContentsSize = size;
        //     node.Read(this, mCursor);
        //     return node;
        // }

        case NodeType::Integer: {
            IntegerNode node;
            node.mType = type;
            node.mContentsSize = size;
            node.Read(mCursor);
            return node;
        }

        case NodeType::String: {
            StringNode node;
            node.mType = type;
            node.mContentsSize = size;
            node.Read(mCursor);
            return node;
        }

        case NodeType::StringBank: {
            StringBank node;
            node.mType = type;
            node.mContentsSize = size;
            node.Read(mCursor);
            return node;
        };

        default: {
            NodeBase node;
            node.mType = type;
            node.mContentsSize = size;

            // skip unk bytes
            mCursor.Seek(size);

            return node;
        }
    }
}

void BSON::Parse() {
    // skip header
    mCursor.SeekTo(0x10);

    size_t bankIndex = -1u;
    while (mCursor.Pos() < mFilesize) {
        Node node = ReadNode();
        mNodes.push_back(node);

        if (const StringBank* pBank = std::get_if<StringBank>(&node)) {
            mStrings = pBank->mStrings;
            bankIndex = mNodes.size() - 1;
        } // there should only ever be one string bank
    }

    StringBank* pBank = nullptr;
    if (bankIndex != -1u) {
        pBank = &std::get<StringBank>(mNodes[bankIndex]);
    }

    std::cout << "test\n";
    // print each node

    for (Node& node : mNodes) {
        std::visit([pBank](const auto& arg) {
        std::string type = arg.GetType();
        std::string str = arg.Format(pBank);
        std::cout << std::format("{}: {}", type, str) << std::endl; 
        }, node);
    }
}
