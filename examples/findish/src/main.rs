use miette::Result;
use std::process;
use structopt::StructOpt;
use treeish::Treeish;

#[derive(Debug, StructOpt)]
#[structopt(rename_all = "kebab-case")]
struct Program {
    treeish: String,
    #[structopt(flatten)]
    options: OptionGroup,
}

impl Program {
    pub fn run(&mut self) -> Result<()> {
        let treeish = Treeish::new(self.treeish.as_ref())?;
        for entry in treeish.walk() {
            if let Ok(entry) = entry {
                println!("{:?}", entry.path());
            }
        }
        Ok(())
    }
}

#[derive(Debug, StructOpt)]
#[structopt(rename_all = "kebab-case")]
struct OptionGroup {}

fn main() {
    process::exit(match Program::from_args().run() {
        Ok(_) => 0,
        Err(report) => {
            eprintln!("{:?}", report);
            1
        },
    })
}
