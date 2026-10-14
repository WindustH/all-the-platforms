//! Trains: the day's trains between two stations or cities from the list
//! page of `trains.ctrip.com` (schedule, seats, fares, tickets left), and
//! plans with one change (`14666/json/trainGetTransferRouteForApp`: train
//! and train, or a flight and a train). Ctrip resells 12306's tickets; atp
//! only reads.

use atp_core::text::encode_uri_component;
use atp_core::{Error, Page, PageReq, Post, Result, ValueExt, json, paging};
use jiff::civil::Date;

use crate::api::{Api, Site, TRAINS};
use crate::parse;

/// Which plans the transfer service puts together (as the app asks).
const TRANSFER_KINDS: &str = "TwoTransferBuyBuyFill,TwoTransferTripleCombineSeat,\
  TripleTransferTwoCombineSeat,App3Transfer3CombineSeat";

/// `trains FROM TO`: the day's direct trains.
pub async fn trains(api: &Api, from: &str, to: &str, date: Date) -> Result<Page<Post>> {
  let url = format!(
    "{TRAINS}/webapp/train/list?ticketType=1&dStationName={}&aStationName={}&dDate={date}",
    encode_uri_component(from),
    encode_uri_component(to)
  );
  let html = api.page(&url, Site::Trains).await?;
  let info = parse::next_data(&html)
    .map(|d| d.at("props.pageProps.initialState.trainSearchInfo").clone())
    .filter(|i| i.is_object())
    .ok_or_else(|| Error::upstream("no trains in Ctrip's list page"))?;
  for (side, name) in [("Depart", from), ("Arrive", to)] {
    if info.bool(&format!("noMatchChinese{side}")) == Some(true) {
      return Err(Error::not_found(format!(
        "Ctrip knows no station or city {name}"
      )));
    }
  }
  let day = date.to_string();
  let items: Vec<Post> = info
    .list("trainInfoList")
    .iter()
    .filter_map(|t| parse::train(t, &day))
    .collect();
  Ok(Page::last(items))
}

/// `transfers FROM TO`: the day's plans with one change, in Ctrip's
/// order. The service answers every plan at once.
pub async fn transfers(
  api: &Api,
  from: &str,
  to: &str,
  date: Date,
  page: &PageReq,
) -> Result<Page<Post>> {
  let mut head = api.head();
  head["extension"] = json!([{ "name": "x-train-bizKeys", "value": TRANSFER_KINDS }]);
  let body = json!({
    "DepartureName": from, "ArrivalName": to, "DepartureCode": "", "ArrivalCode": "",
    "DepartureDate": date.strftime("%Y%m%d").to_string(),
    "OptionalFilter": {
      "HasTicket": false, "TransferStation": "", "TransferType": "",
      "MinTransferMinutes": 0, "MaxTransferMinutes": 0, "DepartureTrainTimePeriod": [],
      "ArrivalTrainTimePeriod": [], "TransferInSameStation": false, "TrainType": "",
      "SortType": ""
    },
    "ExtendList": [{ "Key": "transferPosition", "Value": "transferTab" }],
    "SearchType": 0,
    "QueryType": "Train",
    "head": head
  });
  let v = api
    .soa("14666/json/trainGetTransferRouteForApp", Site::Trains, body)
    .await?;
  let items: Vec<Post> = v
    .list("TransferLines")
    .iter()
    .filter_map(parse::transfer)
    .collect();
  Ok(paging::slice(items, page))
}
