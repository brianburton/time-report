use anyhow::Result;
use anyhow::anyhow;
use derive_getters::Getters;
use im::Vector;

#[derive(Clone, Debug, Getters)]
pub struct MenuItem<T: Clone + Copy> {
    description: String,
    display: String,
    key: char,
    value: T,
}

impl<T: Clone + Copy> MenuItem<T> {
    pub fn new(value: T, name: &str, description: &str, key: char) -> MenuItem<T> {
        let display_string = format!("{}:{}", key, name);
        MenuItem {
            description: description.to_string(),
            display: display_string,
            key,
            value,
        }
    }
}

#[derive(Clone, Debug, Getters)]
pub struct Menu<T: Clone + Copy> {
    items: Vector<MenuItem<T>>,
    selected_index: usize,
}

impl<T: Clone + Copy> Menu<T> {
    pub fn new(items: Vector<MenuItem<T>>) -> Result<Menu<T>> {
        if items.is_empty() {
            Err(anyhow!("Menu: No menu items found"))
        } else {
            Ok(Menu {
                items,
                selected_index: 0,
            })
        }
    }

    fn new_selection(&self, selected_index: usize) -> Self {
        Menu {
            items: self.items.clone(),
            selected_index,
        }
    }

    pub fn select(&self, c: char) -> Option<Self> {
        for (index, item) in self.items.iter().enumerate() {
            if item.key == c {
                return Some(self.new_selection(index));
            }
        }
        None
    }

    pub fn left(&self) -> Self {
        let new_index = match self.selected_index {
            0 => self.items.len() - 1,
            _ => self.selected_index - 1,
        };
        self.new_selection(new_index)
    }

    pub fn right(&self) -> Self {
        let mut new_index = self.selected_index + 1;
        if new_index >= self.items.len() {
            new_index = 0
        }
        self.new_selection(new_index)
    }

    pub fn description(&self) -> &str {
        self.items[self.selected_index].description.as_str()
    }

    pub fn value(&self) -> T {
        self.items[self.selected_index].value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use im::vector;

    #[derive(Copy, Clone, Debug, PartialEq)]
    enum MenuValue {
        Append,
        Reload,
        Quit,
    }

    #[test]
    fn test_menu_item() {
        let item = MenuItem::new(
            MenuValue::Append,
            "Append",
            "Add current date to the file.",
            'A',
        );
        assert_eq!(item.display, "A:Append");
        assert_eq!(item.description, "Add current date to the file.");
        assert_eq!(item.key, 'A');
    }

    #[test]
    fn test_menu() {
        let menu_items = vector!(
            MenuItem::new(
                MenuValue::Append,
                "Append",
                "Add current date to the file.",
                'a'
            ),
            MenuItem::new(MenuValue::Reload, "Reload", "Force reload of file.", 'r'),
            MenuItem::new(MenuValue::Quit, "Quit", "Quit the program.", 'Q'),
        );
        let mut menu = Menu::new(menu_items.clone()).unwrap();
        for i in 0..menu_items.len() {
            assert_eq!(menu.selected_index, i);
            assert_eq!(menu_items[i].description(), menu.description());
            assert_eq!(menu_items[i].value, menu.value());
            menu = menu.right();
        }
        for i in (0..menu_items.len()).rev() {
            menu = menu.left();
            assert_eq!(menu.selected_index, i);
        }

        menu = menu.new_selection(0).left();
        assert_eq!(menu.selected_index, menu.items.len() - 1);
        menu = menu.right();
        assert_eq!(menu.selected_index, 0);

        assert_eq!(None, menu.select('x').map(|x| x.value()));
        assert_eq!(Some(MenuValue::Reload), menu.select('r').map(|x| x.value()));
    }
}
