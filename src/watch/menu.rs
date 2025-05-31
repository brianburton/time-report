use anyhow::Result;
use anyhow::anyhow;
use derive_getters::Getters;
use im::Vector;

#[derive(Clone, Getters)]
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

#[derive(Getters)]
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

    pub fn select(&mut self, c: char) -> Option<T> {
        for (index, item) in self.items.iter().enumerate() {
            if item.key == c {
                self.selected_index = index;
                return Some(item.value);
            }
        }
        None
    }

    pub fn left(&mut self) {
        if self.selected_index == 0 {
            self.selected_index = self.items.len() - 1;
        } else {
            self.selected_index -= 1;
        }
    }

    pub fn right(&mut self) {
        self.selected_index += 1;
        if self.selected_index >= self.items.len() {
            self.selected_index = 0
        }
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

    #[derive(Copy, Clone)]
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
            menu.right();
        }
        for i in (0..menu_items.len()).rev() {
            menu.left();
            assert_eq!(menu.selected_index, i);
        }
    }
}
