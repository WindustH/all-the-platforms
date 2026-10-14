//! What all-the-platforms's Alibaba sites (Xianyu, Taobao) share: the mtop
//! h5 gateway with its `_m_h5_tk` signing and result codes ([`mtop`]), the
//! DingTalk IMPaaS chat socket ([`impaas`]) whose pushes come as MessagePack
//! ([`msgpack`]), the chat requests and parsers on top of it ([`chats`]),
//! the Havana passport's QR login ([`havana`]), the cookies that name the
//! account ([`account`]) and the text forms both sites write prices, links
//! and times in ([`text`]).

pub mod account;
pub mod chats;
pub mod havana;
pub mod impaas;
pub mod msgpack;
pub mod mtop;
pub mod text;
