use ribbit::*;

fn main() -> Result<(), anyhow::Error> {
    // let res = summary(Server::EU)?;
    // dbg!(res);

    let products = summary(Server::EU)?;
    dbg!(products);

    let res = versions(Server::EU, "catalogs")?;
    dbg!(res);

    let res = cdns(Server::EU, "catalogs")?;
    dbg!(res);

    // let res = execute_ribbit_command(Server::EU, Command::ProductBGDL { product: "wow" })?;
    // let res = String::from_utf8(res)?;
    // println!("{}", res);

    Ok(())
}
